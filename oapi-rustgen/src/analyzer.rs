use std::collections::BTreeMap;

use convert_case::{Case, Casing};
use derive_more::{Display, Error};
use http::Method;
use jsonptr::{Pointer, Resolve, Token};

use crate::{
    join_ptr,
    spec::{MediaType, ObjectOrReference, ParameterLocation, Schema, SchemaType, Spec},
    DefaultRenamer, Renamer,
};

#[derive(Debug, Clone)]
pub struct CollectedSchema {
    location: Pointer,
    name: String,
    schema: Schema,
}

impl CollectedSchema {
    pub fn location(&self) -> &Pointer {
        &self.location
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn schema(&self) -> &Schema {
        &self.schema
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum SegmentOrParameter {
    Segment(String),
    Parameter(String),
}

impl SegmentOrParameter {
    fn is_empty(&self) -> bool {
        match self {
            Self::Segment(s) => s.is_empty(),
            Self::Parameter(s) => s.is_empty(),
        }
    }

    pub fn as_segment(&self) -> Option<&String> {
        match self {
            SegmentOrParameter::Segment(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_parameter(&self) -> Option<&String> {
        match self {
            SegmentOrParameter::Parameter(s) => Some(s),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ParameterDef {
    name: String,
    location: ParameterLocation,
    schema_type: String,
}

impl ParameterDef {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn schema_type(&self) -> &str {
        &self.schema_type
    }

    pub fn location(&self) -> ParameterLocation {
        self.location
    }
}

#[derive(Debug, Clone)]
pub struct OperationDef {
    name: String,
    method: Method,
    path: Vec<SegmentOrParameter>,
    parameters: BTreeMap<(String, ParameterLocation), ParameterDef>,
    request_body: Option<String>,
    /// Name of the enum containing all of the possible resposes
    response: String,
    responses: BTreeMap<String, Option<String>>,
}

impl OperationDef {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn method(&self) -> &Method {
        &self.method
    }

    pub fn path(&self) -> &[SegmentOrParameter] {
        &self.path
    }

    pub fn path_params(&self) -> Box<dyn Iterator<Item = (&String, &ParameterDef)> + '_> {
        Box::new(
            self.path()
                .iter()
                .filter_map(|o| match o {
                    SegmentOrParameter::Parameter(p) => Some(p),
                    _ => None,
                })
                .map(|v| {
                    (
                        v,
                        self.parameters
                            .get(&(v.clone(), ParameterLocation::Path))
                            .expect("path parameter should exist"),
                    )
                }),
        )
    }

    pub fn param_by_name(&self, name: &str, location: ParameterLocation) -> Option<&ParameterDef> {
        self.parameters
            .iter()
            .find(|((n, l), _)| n == name && l == &location)
            .map(|(_, p)| p)
    }

    pub fn request_body(&self) -> Option<&str> {
        self.request_body.as_deref()
    }

    pub fn response(&self) -> &str {
        &self.response
    }

    pub fn responses(&self) -> &BTreeMap<String, Option<String>> {
        &self.responses
    }

    pub fn has_default_response(&self) -> bool {
        self.responses
            .iter()
            .any(|(status_code, _)| status_code.as_str() == "default")
    }

    pub fn has_any_response_body(&self) -> bool {
        self.responses.iter().any(|(_, v)| v.is_some())
    }
}

pub struct Analyzer {
    renamer: Box<dyn Renamer>,
}

impl Analyzer {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn with_renamer(&mut self, renamer: Box<dyn Renamer>) {
        self.renamer = renamer;
    }

    pub fn run(self, spec: &str) -> Result<AnalysisResult, AnalysisError> {
        let spec: Spec = serde_json::de::from_str(spec).map_err(AnalysisError::Deserialization)?;
        let schemas = collect_types_to_generate(&spec);
        Ok(AnalysisResult {
            renamer: self.renamer,
            spec,
            schemas,
        })
    }
}

impl Default for Analyzer {
    fn default() -> Self {
        Self {
            renamer: Box::<DefaultRenamer>::default(),
        }
    }
}

pub struct AnalysisResult {
    renamer: Box<dyn Renamer>,
    spec: Spec,
    schemas: Vec<CollectedSchema>,
}

impl AnalysisResult {
    pub fn renamer(&self) -> &Box<dyn Renamer> {
        &self.renamer
    }

    pub fn spec(&self) -> &Spec {
        &self.spec
    }

    pub fn schemas(&self) -> &[CollectedSchema] {
        &self.schemas
    }

    pub fn find_schema(&self, ptr: &Pointer) -> Option<&CollectedSchema> {
        self.schemas().iter().find(|s| s.location() == ptr)
    }

    pub fn name_type(&self, ptr: &Pointer, schema: &ObjectOrReference<Schema>) -> String {
        if let Some(ty) = self.find_schema(ptr) {
            return ty.name.clone();
        }

        let make_nullable = match schema {
            ObjectOrReference::Object(schema) if schema.nullable == Some(true) => {
                |s| format!("Option<{}>", s)
            }
            _ => |s| s,
        };

        match schema {
            ObjectOrReference::Ref { ref_path } => self
                .find_schema(ref_path)
                .unwrap_or_else(|| panic!("reference `{}` should exist as schema", ref_path))
                .name()
                .to_owned(),
            ObjectOrReference::Object(schema) if schema.schema_type == Some(SchemaType::Object) => {
                make_nullable(
                    self.find_schema(ptr)
                        .unwrap_or_else(|| {
                            panic!("property `{}` should exist as schema", ptr)
                        })
                        .name()
                        .to_owned(),
                )
            }
            ObjectOrReference::Object(schema) if schema.schema_type == Some(SchemaType::Array) => {
                if let Some(items) = &schema.items {
                    let ptr = join_ptr!(ptr, "items");

                    make_nullable(format!("Vec<{}>", self.name_type(&ptr, items)))
                } else {
                    make_nullable("Vec<serde_json::Value>".to_owned())
                }
            }
            ObjectOrReference::Object(schema)
                if schema.schema_type == Some(SchemaType::Integer) =>
            {
                if let Some("int32") = schema.format.as_deref() {
                    make_nullable("i32".to_owned())
                } else {
                    make_nullable("i64".to_owned())
                }
            }
            ObjectOrReference::Object(schema) if schema.schema_type == Some(SchemaType::Number) => {
                if let Some("float") = schema.format.as_deref() {
                    make_nullable("f32".to_owned())
                } else {
                    make_nullable("f64".to_owned())
                }
            }
            ObjectOrReference::Object(schema) if schema.schema_type == Some(SchemaType::String) => {
                make_nullable("String".to_owned())
            }
            ObjectOrReference::Object(schema)
                if schema.schema_type == Some(SchemaType::Boolean) =>
            {
                make_nullable("bool".to_owned())
            }
            _ => "serde_json::Value".to_owned(),
        }
    }

    pub fn operations(&self) -> Vec<OperationDef> {
        let paths_ptr = Pointer::new(&[Token::new("paths")]);
        let spec_value = serde_json::to_value(&self.spec).expect("schema should be serializable");
        let mut operations = vec![];

        for (path, method, operation) in self.spec.operations() {
            let ptr = join_ptr!(paths_ptr, &path, method.to_string().to_lowercase());
            let operation_name = self.renamer.name_operation(&spec_value, &ptr);
            let path = path.chars().fold(vec![], |mut memo, c| {
                if c == '/' {
                    if !memo.is_empty() {
                        memo.push(SegmentOrParameter::Segment("".to_owned()));
                    }
                    return memo;
                }
                if c == '{' {
                    if let Some(s) = memo.pop() {
                        if s.is_empty() {
                            memo.push(SegmentOrParameter::Parameter("".to_owned()));
                            return memo;
                        }
                    }
                    panic!("parameter start is incorrect for path `{}`", &path);
                }
                if c == '}' {
                    if let Some(SegmentOrParameter::Parameter(_)) = memo.last() {
                        return memo;
                    }
                    panic!("parameter end is incorrect for path `{}`", &path);
                }

                match memo.pop() {
                    None => {
                        memo.push(SegmentOrParameter::Segment(c.to_string()));
                    }
                    Some(SegmentOrParameter::Segment(mut s)) => {
                        s.push(c);
                        memo.push(SegmentOrParameter::Segment(s));
                    }
                    Some(SegmentOrParameter::Parameter(mut s)) => {
                        s.push(c);
                        memo.push(SegmentOrParameter::Parameter(s));
                    }
                }

                memo
            });
            let parameters: BTreeMap<_, _> = operation
                .parameters
                .iter()
                .enumerate()
                .map(|(i, p_or_ref)| {
                    let ptr = join_ptr!(ptr, "parameters", i.to_string(), "schema");
                    let param = p_or_ref.resolve(&self.spec).expect("should be resolvable");
                    let schema =
                        ObjectOrReference::Object(param.schema.expect("should have a schema"));
                    match p_or_ref {
                        ObjectOrReference::Object(s) => (
                            (s.name.clone(), s.location),
                            ParameterDef {
                                name: self.renamer().name_parameter(&s.name),
                                schema_type: self.name_type(&ptr, &schema),
                                location: s.location,
                            },
                        ),
                        ObjectOrReference::Ref { ref_path } => {
                            let s = self.find_schema(ref_path).unwrap_or_else(|| panic!("reference `{}` should exist as schema", ref_path));
                            (
                                (param.name.clone(), param.location),
                                ParameterDef {
                                    name: self.renamer().name_parameter(&param.name),
                                    schema_type: s.name().to_owned(),
                                    location: param.location,
                                },
                            )
                        }
                    }
                })
                .collect();
            let request_body = operation.request_body.as_ref().and_then(|b| match b {
                ObjectOrReference::Object(s) => {
                    let ptr = join_ptr!(
                        &ptr,
                        "request_body",
                        "content",
                        "application/json",
                        "schema"
                    );
                    s.content
                        .get("application/json")
                        .and_then(|v| v.schema.as_ref())
                        .map(|schema| self.name_type(&ptr, schema))
                }
                ObjectOrReference::Ref { ref_path } => {
                    let s = self.find_schema(ref_path).unwrap_or_else(|| panic!("reference `{}` should exist as schema", ref_path));
                    Some(s.name().to_owned())
                }
            });
            let responses: BTreeMap<_, _> = operation
                .responses
                .iter()
                .map(|(status_code, r_or_ref)| match r_or_ref {
                    ObjectOrReference::Object(r) => {
                        let ptr = join_ptr!(
                            &ptr,
                            "responses",
                            status_code,
                            "content",
                            "application/json",
                            "schema"
                        );
                        let s = r
                            .content
                            .get("application/json")
                            .and_then(|v| v.schema.as_ref())
                            .map(|schema| self.name_type(&ptr, schema));
                        (status_code.clone(), s)
                    }
                    ObjectOrReference::Ref { ref_path } => {
                        let s = self.find_schema(ref_path).map(|s| s.name.clone());
                        (status_code.clone(), s)
                    }
                })
                .collect();
            let response = if responses.len() == 1 {
                responses
                    .iter()
                    .next()
                    .expect("single item")
                    .1
                    .as_ref()
                    .cloned()
                    .unwrap_or_else(|| "()".to_owned())
            } else {
                format!("{}Response", operation_name)
            };

            operations.push(OperationDef {
                name: operation_name.to_case(Case::Snake),
                method,
                path,
                parameters,
                request_body,
                response,
                responses,
            });
        }

        operations
    }
}

#[derive(Debug, Display, Error)]
pub enum AnalysisError {
    #[display(fmt = "Failed to deserialize openapi spec: {}", _0)]
    Deserialization(serde_json::Error),
}

fn collect_initial_types_to_generate(spec: &Spec) -> Vec<Pointer> {
    let mut types_to_check = vec![];
    let root_ptr = Pointer::root();
    let components_ptr = join_ptr!(root_ptr, "components");

    spec.components.schemas.iter().for_each(|(name, _)| {
        let type_path = join_ptr!(components_ptr, "schemas", name);
        types_to_check.push(type_path);
    });
    spec.components
        .responses
        .iter()
        .flat_map(|(name, response)| response.object().map(|r| (name, r)))
        .map(|(s, r)| (s, collect_initial_types_from_media_types(&r.content)))
        .for_each(|(name, response_media_types)| {
            response_media_types.for_each(|(media_type, _)| {
                let type_path = join_ptr!(
                    components_ptr,
                    "responses",
                    name,
                    "content",
                    media_type,
                    "schema"
                );
                types_to_check.push(type_path);
            })
        });
    spec.components
        .parameters
        .iter()
        .flat_map(|(name, parameter)| parameter.object().map(|o| (name, o)))
        .flat_map(|(name, p)| p.schema.as_ref().map(|s| (name, s)))
        .for_each(|(name, _)| {
            let type_path = join_ptr!(components_ptr, "parameters", name, "schema");
            types_to_check.push(type_path);
        });
    spec.components
        .request_bodies
        .iter()
        .flat_map(|(name, parameter)| parameter.object().map(|o| (name, o)))
        .flat_map(|(name, r)| {
            collect_initial_types_from_media_types(&r.content).map(|m| (name.clone(), m))
        })
        .for_each(|(name, (media_type, _))| {
            let type_path = join_ptr!(
                components_ptr,
                "requestBodies",
                name,
                "content",
                media_type,
                "schema"
            );
            types_to_check.push(type_path);
        });

    spec.operations().for_each(|(path, method, operation)| {
        let operation_ptr = Pointer::new([
            Token::new("paths"),
            path.into(),
            // TODO: Method is not always lowercase
            method.to_string().to_lowercase().into(),
        ]);

        operation
            .parameters
            .iter()
            .enumerate()
            .flat_map(|(i, p)| p.object().map(|o| (i, o)))
            .flat_map(|(i, p)| p.schema.as_ref().map(|s| (i, s)))
            .for_each(|(i, _)| {
                let type_path = join_ptr!(operation_ptr, "parameters", i.to_string(), "schema");
                types_to_check.push(type_path);
            });

        operation
            .request_body
            .as_ref()
            .and_then(|r| r.object())
            .into_iter()
            .flat_map(|r| collect_initial_types_from_media_types(&r.content))
            .for_each(|(media_type, _)| {
                let type_path = join_ptr!(
                    operation_ptr,
                    "requestBody",
                    "content",
                    media_type,
                    "schema"
                );
                types_to_check.push(type_path);
            });

        operation
            .responses
            .iter()
            .flat_map(|(s, r)| r.object().map(|o| (s, o)))
            .map(|(s, r)| (s, collect_initial_types_from_media_types(&r.content)))
            .for_each(|(status_code, response_media_types)| {
                response_media_types.for_each(|(media_type, _)| {
                    let type_path = join_ptr!(
                        operation_ptr,
                        "responses",
                        status_code,
                        "content",
                        media_type,
                        "schema"
                    );
                    types_to_check.push(type_path);
                })
            });
    });

    types_to_check
}

fn collect_initial_types_from_media_types(
    m: &BTreeMap<String, MediaType>,
) -> impl Iterator<Item = (&String, &Schema)> {
    m.iter()
        .flat_map(|(m, o)| o.schema.as_ref().map(|s| (m, s)))
        .flat_map(|(m, s)| s.object().map(|o| (m, o)))
}

fn collect_types_to_generate(spec: &Spec) -> Vec<CollectedSchema> {
    let renamer = DefaultRenamer {};
    // Initialize types to check
    let mut types_to_check = collect_initial_types_to_generate(spec);
    let spec = serde_json::to_value(spec).expect("schema should be serializable");
    let mut collected_types = vec![];

    while let Some(type_ptr) = types_to_check.pop() {
        let schema = spec
            .resolve(&type_ptr)
            .expect("types to check should be resolvable");
        let schema: ObjectOrReference<Schema> =
            serde_json::from_value(schema.clone()).expect("should be a schema");

        match schema {
            ObjectOrReference::Object(schema) if !schema.any_of.is_empty() || !schema.all_of.is_empty() || !schema.one_of.is_empty() => {
                collected_types.push(CollectedSchema {
                    location: type_ptr.clone(),
                    name: renamer.name_type(&spec, &type_ptr),
                    schema: schema.clone(),
                });
                for (i, _) in schema.any_of.iter().enumerate() {
                    let ptr = join_ptr!(&type_ptr, "anyOf", i.to_string());
                    types_to_check.push(ptr);
                }
                for (i, _) in schema.all_of.iter().enumerate() {
                    let ptr = join_ptr!(&type_ptr, "allOf", i.to_string());
                    types_to_check.push(ptr);
                }
                for (i, _) in schema.one_of.iter().enumerate() {
                    let ptr = join_ptr!(&type_ptr, "oneOf", i.to_string());
                    types_to_check.push(ptr);
                }
            },
            ObjectOrReference::Object(schema) if schema.schema_type == Some(SchemaType::Object) => {
                collected_types.push(CollectedSchema {
                    location: type_ptr.clone(),
                    name: renamer.name_type(&spec, &type_ptr),
                    schema: schema.clone(),
                });
                for (name, schema) in &schema.properties {
                    if let ObjectOrReference::Object(_) = schema {
                        types_to_check.push(join_ptr!(&type_ptr, "properties", name));
                    }
                }
            }
            ObjectOrReference::Object(schema) if schema.schema_type == Some(SchemaType::Array) => {
                if let Some(ObjectOrReference::Object(_)) = schema.items.as_deref() {
                    types_to_check.push(join_ptr!(type_ptr, "items"));
                }
            }
            _ => {}
        }
    }

    collected_types
}
