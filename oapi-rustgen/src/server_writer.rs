use std::collections::BTreeMap;

use derive_more::{Display, Error};
use genco::{prelude::rust::Tokens, quote, tokens::quoted};
use http::Method;

use crate::{AnalysisResult, ClientWriter, OperationDef, SegmentOrParameter};

#[derive(Debug, Display, Error)]
pub enum ServerWriterError {}

pub struct ServerWriter<'a> {
    analysis: &'a AnalysisResult,
}

impl<'a> ServerWriter<'a> {
    pub fn new(analysis: &'a AnalysisResult) -> Self {
        ServerWriter { analysis }
    }

    pub fn write(&self) -> Result<Tokens, ServerWriterError> {
        let mut tokens = Tokens::new();

        tokens.append(quote! { use futures::StreamExt; });
        tokens.append(quote! { use std::str::FromStr; });
        tokens.line();

        let trait_def: Tokens = quote! {
            #[async_trait::async_trait(?Send)]
            pub trait Handlers {
                type Error: std::fmt::Debug;

                $(for o in &self.analysis.operations() =>
                    $(ClientWriter::write_operation_function_signature(o));
                )
            }
        };
        tokens.append(&trait_def);
        tokens.line();

        let operations_by_path =
            self.analysis
                .operations()
                .into_iter()
                .fold(BTreeMap::default(), |mut memo, o| {
                    let entry = memo.entry(o.path().to_owned()).or_insert(vec![]);
                    entry.push(o);
                    memo
                });
        let trait_impl: Tokens = quote! {
            pub async fn handler<T, E>(
                req: actix_web::HttpRequest,
                mut payload: actix_web::web::Payload,
            ) -> Result<actix_web::HttpResponse, actix_web::Error>
            where
                T: Handlers + actix_web::FromRequest<Error = E>,
                E: std::fmt::Debug
            {
                let handlers = T::extract(&req).await.expect("handler data should be set");
                let method = req.method();
                let path: Vec<_> = req.path().split('/').skip(1).collect();
                let mut body = actix_web::web::BytesMut::new();
                while let Some(item) = payload.next().await {
                    body.extend_from_slice(&item.expect("should read"));
                }

                $(for (path, operations) in &operations_by_path => if let $(self.get_operation_path_match(path)) {
                    $(for o in operations =>
                        if method == $(self.get_actix_method(o)) {
                            $(for (_, p) in o.path_params() => let $(p.name()) = $(p.schema_type())::from_str($(p.name())).expect("should deserialize");)
                            $(if let Some(body) = o.request_body() {
                                let body: $(body) = serde_json::from_slice(&body).expect("body should deserialize");
                            })
                            let response = handlers.$(o.name())($(for (_, p) in o.path_params() => $(p.name()), )$(if o.request_body().is_some() { body })).await.expect("should execute");

                            $(self.match_responses(o))
                            
                        }
                    )
                })

                todo!();
            }
        };
        tokens.append(&trait_impl);
        tokens.line();

        Ok(tokens)
    }

    fn get_operation_path_match(&self, path: &[SegmentOrParameter]) -> Tokens {
        quote! { &[$(for s in path join (, ) => $(match s {
            SegmentOrParameter::Segment(s) => $(quoted(s)),
            SegmentOrParameter::Parameter(p) => $(p),
        }))] = path.as_slice() }
    }

    fn get_actix_method(&self, o: &OperationDef) -> Tokens {
        match *o.method() {
            Method::CONNECT => quote! { actix_web::http::Method::CONNECT },
            Method::DELETE => quote! { actix_web::http::Method::DELETE },
            Method::GET => quote! { actix_web::http::Method::GET },
            Method::HEAD => quote! { actix_web::http::Method::HEAD },
            Method::OPTIONS => quote! { actix_web::http::Method::OPTIONS },
            Method::PATCH => quote! { actix_web::http::Method::PATCH },
            Method::POST => quote! { actix_web::http::Method::POST },
            Method::PUT => quote! { actix_web::http::Method::PUT },
            Method::TRACE => quote! { actix_web::http::Method::TRACE },
            _ => panic!("unknown method `{:?}` for actix_web", o.method()),
        }
    }

    fn match_responses(&self, operation: &OperationDef) -> Tokens {
        if operation.responses().len() == 1 {
            let (status_code, _) = operation.responses().first_key_value().expect("length 1");
            let status_code: u16 = status_code.parse().unwrap_or(500);
            if operation.response() == "()" {
                quote! {
                    return Ok(actix_web::HttpResponseBuilder::new(actix_web::http::StatusCode::from_u16($(status_code)).expect("valid status code")).finish());
                }
            } else {
                quote! {
                    return Ok(actix_web::HttpResponseBuilder::new(actix_web::http::StatusCode::from_u16($(status_code)).expect("valid status code")).json(body));
                }
            }
        } else {
            let match_arms: Vec<_> = operation.responses().iter().map(|(status_code, schema)| {
                let status_code_i: u16 = status_code.parse().unwrap_or(500);
                quote! {
                    $(operation.response())::S$(status_code)$(if schema.is_some() { (body) }) => {
                        return Ok(actix_web::HttpResponseBuilder::new(actix_web::http::StatusCode::from_u16($(status_code_i))
                            .expect("valid status code"))
                            $(if schema.is_some() { .json(body) } else { .finish() }));
                    },
                }
            }).collect();
            quote! {
                match response {
                    $(for match_arm in &match_arms => $(match_arm))
                }
            }
        }
    }
}
