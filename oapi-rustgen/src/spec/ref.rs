use std::str::FromStr;

use derive_more::{Display, Error};
use jsonptr::Pointer;
use log::trace;
use once_cell::sync::Lazy;
use regex::Regex;
use serde::{de::Error, Deserialize, Serialize};
use serde_json::Value;

use super::Spec;

static RE_REF: Lazy<Regex> = Lazy::new(|| {
    Regex::new("^(?P<source>[^#]*)#/components/(?P<type>[^/]+)/(?P<name>.+)$").unwrap()
});

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(untagged)]
pub enum ObjectOrReference<T>
where
    T: serde::de::DeserializeOwned,
{
    Ref {
        #[serde(rename = "$ref")]
        ref_path: Pointer,
    },
    Object(T),
}

impl<'de, T: serde::de::DeserializeOwned> serde::de::Deserialize<'de> for ObjectOrReference<T> {
    fn deserialize<D>(deserializer: D) -> Result<ObjectOrReference<T>, D::Error>
    where
        D: serde::de::Deserializer<'de>,
    {
        let s: Value = Deserialize::deserialize(deserializer)?;
        if let Some(ref_path) = s.get("$ref") {
            match ref_path {
                Value::String(ref_path) => Pointer::from_str(ref_path)
                    .map(|ref_path| ObjectOrReference::Ref { ref_path })
                    .map_err(D::Error::custom),
                _ => Err(D::Error::custom("$ref is not of type string")),
            }
        } else {
            serde_json::from_value(s.clone())
                .map(|v| ObjectOrReference::Object(v))
                .map_err(D::Error::custom)
        }
    }
}

impl<T: serde::de::DeserializeOwned> ObjectOrReference<T>
where
    T: FromRef,
{
    pub fn resolve(&self, spec: &Spec) -> Result<T, RefError> {
        match self {
            Self::Object(component) => Ok(component.clone()),
            Self::Ref { ref_path } => T::from_ref(spec, ref_path),
        }
    }

    pub fn object(&self) -> Option<&T> {
        match self {
            ObjectOrReference::Object(v) => Some(v),
            ObjectOrReference::Ref { .. } => None,
        }
    }

    pub fn reference(&self) -> Option<&str> {
        match self {
            ObjectOrReference::Ref { ref_path } => Some(ref_path),
            ObjectOrReference::Object(_) => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Display, Error)]
pub enum RefError {
    #[display(fmt = "Invalid type: {}", _0)]
    InvalidType(#[error(not(source))] String),

    #[display(fmt = "Mismatched type: cannot reference a {} as a {}", _0, _1)]
    MismatchedType(RefType, RefType),

    // TODO: use some kind of path structure
    #[display(fmt = "Unresolvable path: {}", _0)]
    Unresolvable(#[error(not(source))] String),
}

#[derive(Copy, Clone, Debug, PartialEq, Display)]
pub enum RefType {
    Schema,
    Response,
    Parameter,
    Example,
    RequestBody,
    Header,
    SecurityScheme,
    Link,
    Callback,
}

impl FromStr for RefType {
    type Err = RefError;

    fn from_str(typ: &str) -> Result<Self, Self::Err> {
        Ok(match typ {
            "schemas" => Self::Schema,
            "responses" => Self::Response,
            "parameters" => Self::Parameter,
            "examples" => Self::Example,
            "requestBodies" => Self::RequestBody,
            "headers" => Self::Header,
            "securitySchemes" => Self::SecurityScheme,
            "links" => Self::Link,
            "callbacks" => Self::Callback,
            typ => return Err(RefError::InvalidType(typ.to_owned())),
        })
    }
}

#[derive(Debug, Clone)]
pub struct Ref {
    pub source: String,
    pub kind: RefType,
    pub name: String,
}

impl FromStr for Ref {
    type Err = RefError;

    fn from_str(path: &str) -> Result<Self, Self::Err> {
        let parts = RE_REF.captures(path).unwrap();

        trace!("creating Ref: {}/{}", &parts["type"], &parts["name"]);

        Ok(Self {
            source: parts["source"].to_owned(),
            kind: parts["type"].parse()?,
            name: parts["name"].to_owned(),
        })
    }
}

pub trait FromRef: Clone {
    fn from_ref(spec: &Spec, path: &str) -> Result<Self, RefError>;
}
