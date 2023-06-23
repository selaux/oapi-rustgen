use derive_more::{Display, Error};
use genco::{prelude::rust::Tokens, quote, tokens::quoted};
use jsonptr::Resolve;
use std::collections::HashSet;

use crate::{
    analyzer::{AnalysisResult, CollectedSchema},
    join_ptr,
    spec::{ObjectOrReference, Schema, SchemaType},
};

struct PropertyDef {
    name: String,
    json_name: String,
    ptype: String,
}

struct CompositeDef {
    index: usize,
    schema_type: String,
}

#[derive(Debug, Display, Error)]
pub enum TypesWriterError {}

pub struct TypesWriter<'a> {
    analysis: &'a AnalysisResult,
}

impl<'a> TypesWriter<'a> {
    pub fn new(analysis: &'a AnalysisResult) -> Self {
        TypesWriter { analysis }
    }

    pub fn write(&self) -> Result<Tokens, TypesWriterError> {
        let mut tokens = Tokens::new();
        let spec_value =
            serde_json::to_value(self.analysis.spec()).expect("schema should be serializable");
        for ty in self.analysis.schemas() {
            let schema = spec_value
                .resolve(ty.location())
                .expect("types to check should be resolvable");
            let schema: ObjectOrReference<Schema> =
                serde_json::from_value(schema.clone()).expect("should be a schema");
            self.write_type_to_tokens(&mut tokens, ty, &schema);
        }

        for o in self.analysis.operations() {
            if o.responses().len() > 1 {
                let enum_def: Tokens = quote! {
                    #[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
                    pub enum $(o.response()) {
                        $(for (status, r) in o.responses() join (, ) =>
                            S$(status)$(if let Some(r) = r {($(r))})
                        )
                    }
                };
                tokens.append(&enum_def);
                tokens.line();
                tokens.line();
            }
        }
        tokens.line();

        Ok(tokens)
    }

    fn write_type_to_tokens(
        &self,
        tokens: &mut Tokens,
        ty: &CollectedSchema,
        schema: &ObjectOrReference<Schema>,
    ) {
        match schema {
            ObjectOrReference::Object(schema)
                if !schema.any_of.is_empty()
                    || !schema.all_of.is_empty()
                    || !schema.one_of.is_empty() =>
            {
                self.write_composite_to_tokens(tokens, ty, schema);
            }
            ObjectOrReference::Object(schema) if schema.schema_type == Some(SchemaType::Object) => {
                self.write_struct_to_tokens(tokens, ty, schema);
            }
            _ => {
                log::warn!("unsupported schema: {:?}", schema)
            }
        }
    }

    fn write_composite_to_tokens(
        &self,
        tokens: &mut Tokens,
        ty: &CollectedSchema,
        schema: &Schema,
    ) {
        if !schema.any_of.is_empty() {
            let composite_defs = schema.any_of.iter().enumerate().map(|(idx, schema)| {
                let ptr = join_ptr!(ty.location(), "anyOf", idx.to_string());
                CompositeDef {
                    index: idx,
                    schema_type: self.analysis.name_type(&ptr, schema),
                }
            });

            tokens.append(quote! {
                #[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
                pub struct $(ty.name()) {
                    $(for c in composite_defs =>
                        #[serde(flatten)]
                        pub v$(c.index): Option<$(c.schema_type)>,
                    )
                }
            });
            tokens.line();
            return;
        }
        if !schema.all_of.is_empty() {
            let composite_defs = schema.all_of.iter().enumerate().map(|(idx, schema)| {
                let ptr = join_ptr!(ty.location(), "allOf", idx.to_string());
                CompositeDef {
                    index: idx,
                    schema_type: self.analysis.name_type(&ptr, schema),
                }
            });

            tokens.append(quote! {
                #[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
                pub struct $(ty.name()) {
                    $(for c in composite_defs =>
                        #[serde(flatten)]
                        pub v$(c.index): $(c.schema_type),
                    )
                }
            });
            tokens.line();
            return;
        }
        if !schema.one_of.is_empty() {
            let composite_defs = schema.one_of.iter().enumerate().map(|(idx, schema)| {
                let ptr = join_ptr!(ty.location(), "allOf", idx.to_string());
                CompositeDef {
                    index: idx,
                    schema_type: self.analysis.name_type(&ptr, schema),
                }
            });

            tokens.append(quote! {
                #[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
                #[serde(untagged)]
                pub enum $(ty.name()) {
                    $(for c in composite_defs => V$(c.index)(pub $(c.schema_type)),)
                }
            });
            tokens.line();
            return;
        }
        unreachable!("failed to build composite type from schema: {:?}", schema)
    }

    fn write_struct_to_tokens(&self, tokens: &mut Tokens, ty: &CollectedSchema, schema: &Schema) {
        let required_properties: HashSet<_> = schema.required.iter().collect();
        let properties: Vec<_> = schema
            .properties
            .iter()
            .map(|(json_name, schema)| {
                let ptr = join_ptr!(ty.location(), "properties", json_name);
                let field_type = self.analysis.name_type(&ptr, schema);
                let ptype = if required_properties.contains(json_name) {
                    field_type
                } else {
                    format!("Option<{}>", field_type)
                };
                let name = self.analysis.renamer().name_property(json_name);
                let name = syn::parse_str::<syn::Ident>(&name)
                    .map(|i| i.to_string())
                    .unwrap_or_else(|_| format!("r#{}", name));
                PropertyDef {
                    name,
                    json_name: json_name.clone(),
                    ptype,
                }
            })
            .collect();
        let struct_def: Tokens = quote! {
            #[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
            pub struct $(ty.name()) {
                $(for p in properties join (, ) =>
                    $(if p.name != p.json_name {
                        #[serde(rename = $(quoted(p.json_name)))]
                    })
                    pub $(p.name): $(p.ptype)
                )
            }
        };
        tokens.append(&struct_def);
        tokens.line();
        tokens.line();
    }
}
