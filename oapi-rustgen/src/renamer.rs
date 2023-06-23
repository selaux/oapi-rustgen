use std::{str::FromStr, sync::LazyLock};

use convert_case::{Case, Casing};
use jsonptr::{Pointer, Resolve};
use regex::{Captures, Regex};
use serde_json::Value;

static OPERATION_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^/paths/([^/]+)/([^/]+)").expect("OPERATION_REGEX"));
static OPERATION_REQUEST_BODY_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^/paths/([^/]+)/([^/]+)/requestBody/content/([^/]+)/schema$")
        .expect("OPERATION_REQUEST_BODY_REGEX")
});
static OPERATION_PARAMETER_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^/paths/([^/]+)/([^/]+)/parameters/([^/]+)/schema$")
        .expect("OPERATION_PARAMETER_REGEX")
});
static OPERATION_RESPONSE_BODY_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^/paths/([^/]+)/([^/]+)/responses/([^/]+)/content/([^/]+)/schema$")
        .expect("OPERATION_RESPONSE_BODY_REGEX")
});

static SCHEMA_COMPONENT_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^/components/schemas/([^/]+)$").expect("SCHEMA_COMPONENT_REGEX"));
static REQUEST_BODY_COMPONENT_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^/components/requestBodies/([^/]+)/content/([^/]+)/schema$")
        .expect("REQUEST_BODY_COMPONENT_REGEX")
});
static PARAMETER_COMPONENT_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^/components/parameters/([^/]+)/schema$").expect("PARAMETER_COMPONENT_REGEX")
});
static RESPONSE_COMPONENT_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^/components/responses/([^/]+)/content/([^/]+)/schema$")
        .expect("RESPONSE_COMPONENT_REGEX")
});

static SCHEMA_PROPERTY_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new("(.+)/properties/([^/]+)").expect("SCHEMA_PROPERTY_REGEX"));
static SCHEMA_ITEMS_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new("(.+)/items").expect("SCHEMA_ITEMS_REGEX"));
static SCHEMA_COMPOSITE_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new("(.+)/(anyOf|allOf|oneOf)/([0-9]+)").expect("SCHEMA_COMPOSITE_REGEX"));

pub trait Renamer {
    fn name_type(&self, spec: &Value, ptr: &Pointer) -> String;
    fn name_operation(&self, spec: &Value, ptr: &Pointer) -> String;
    fn name_property(&self, name: &str) -> String;
    fn name_parameter(&self, name: &str) -> String;
}

#[derive(Default)]
pub struct DefaultRenamer {}

impl DefaultRenamer {
    fn operation_name_from_captures(&self, spec: &Value, m: &Captures) -> String {
        let path = m.get(1).expect("group").as_str();
        let method = m.get(2).expect("group").as_str();
        let operation =
            Pointer::from_str(&format!("/paths/{}/{}", path, method)).expect("operations");
        self.name_operation(spec, &operation)
    }
}

fn name_from_nth_match(m: &Captures, n: usize) -> String {
    m.get(n)
        .expect("group")
        .as_str()
        .to_owned()
        .to_case(Case::Pascal)
        .chars()
        .filter(|c| c.is_alphanumeric())
        .collect()
}

impl Renamer for DefaultRenamer {
    fn name_operation(&self, spec: &Value, ptr: &Pointer) -> String {
        let operation_id = spec
            .resolve(ptr)
            .ok()
            .and_then(|v: &Value| v.get("operationId"))
            .and_then(|id| id.as_str());
        if let Some(operation_id) = operation_id {
            return operation_id.to_case(Case::Pascal);
        }

        let c = OPERATION_REGEX
            .captures(ptr.as_str())
            .expect("should be in correct format");
        let path = c.get(1).expect("group").as_str();
        let method = c.get(2).expect("group").as_str();
        let path_name = path
            .split("~1")
            .map(|segment| {
                if segment.starts_with('{') && segment.ends_with('}') {
                    // its a parameter
                    &segment[1..segment.len() - 1]
                } else {
                    segment
                }
            })
            .fold("".to_owned(), |a, b| {
                let b: String = b
                    .chars()
                    .enumerate()
                    .flat_map(|(i, c)| {
                        if i == 0 {
                            c.to_uppercase().next()
                        } else {
                            Some(c)
                        }
                    })
                    .collect();
                format!("{}{}", a, b)
            });

        format!(
            "{}{}",
            method.to_case(Case::Pascal),
            path_name.to_case(Case::Pascal)
        )
    }

    fn name_type(&self, spec: &Value, ptr: &Pointer) -> String {
        // First handle all components
        if ptr.starts_with("/components/") {
            if let Some(m) = SCHEMA_COMPONENT_REGEX.captures(ptr.as_str()) {
                return name_from_nth_match(&m, 1);
            }

            if let Some(m) = REQUEST_BODY_COMPONENT_REGEX.captures(ptr.as_str()) {
                return name_from_nth_match(&m, 1);
            }

            if let Some(m) = PARAMETER_COMPONENT_REGEX.captures(ptr.as_str()) {
                return name_from_nth_match(&m, 1);
            }

            if let Some(m) = RESPONSE_COMPONENT_REGEX.captures(ptr.as_str()) {
                return name_from_nth_match(&m, 1);
            }
        }

        // request bodies
        if let Some(m) = OPERATION_REQUEST_BODY_REGEX.captures(ptr.as_str()) {
            let operation_name = self.operation_name_from_captures(spec, &m);

            return format!("{}Request", operation_name,);
        }

        // parameters
        if let Some(m) = OPERATION_PARAMETER_REGEX.captures(ptr.as_str()) {
            let operation_name = self.operation_name_from_captures(spec, &m);
            let parameter_id = m.get(3).expect("group").as_str();

            return format!("{}Parameter{}", operation_name, parameter_id);
        }

        // response bodies
        if let Some(m) = OPERATION_RESPONSE_BODY_REGEX.captures(ptr.as_str()) {
            let response_code = m.get(3).expect("group").as_str();
            let operation_name = self.operation_name_from_captures(spec, &m);

            return format!("{}Response{}", operation_name, response_code);
        }

        // properties
        if let Some(m) = SCHEMA_PROPERTY_REGEX.captures(ptr.as_str()) {
            let parent = m.get(1).expect("group").as_str();
            let parent_name = self.name_type(
                spec,
                &Pointer::from_str(parent).expect("should be a pointer"),
            );
            let attribute_name = m.get(2).expect("group").as_str();
            return format!("{}{}", parent_name, attribute_name.to_case(Case::Pascal));
        }

        // items
        if let Some(m) = SCHEMA_ITEMS_REGEX.captures(ptr.as_str()) {
            let parent = m.get(1).expect("group").as_str();
            let parent_name = self.name_type(
                spec,
                &Pointer::from_str(parent).expect("should be a pointer"),
            );
            return format!("{}Item", parent_name);
        }

        if let Some(m) = SCHEMA_COMPOSITE_REGEX.captures(ptr.as_str()) {
            let parent = m.get(1).expect("group").as_str();
            let index = m.get(3).expect("group").as_str();
            let parent_name = self.name_type(
                spec,
                &Pointer::from_str(parent).expect("should be a pointer"),
            );
            return format!("{}V{}", parent_name, index);
        }

        unreachable!("should be unreachable {:?}", ptr);
    }

    fn name_property(&self, name: &str) -> String {
        name.to_case(Case::Snake)
    }

    fn name_parameter(&self, name: &str) -> String {
        name.to_case(Case::Snake)
    }
}
