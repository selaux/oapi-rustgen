use derive_more::{Display, Error};

use genco::{prelude::rust::Tokens, quote, tokens::quoted};
use http::Method;

use crate::{analyzer::AnalysisResult, spec::ParameterLocation, OperationDef, SegmentOrParameter};

#[derive(Debug, Display, Error)]
pub enum ClientWriterError {}

pub struct ClientWriter<'a> {
    analysis: &'a AnalysisResult,
}

impl<'a> ClientWriter<'a> {
    pub fn new(analysis: &'a AnalysisResult) -> Self {
        ClientWriter { analysis }
    }

    pub fn write(&self) -> Result<Tokens, ClientWriterError> {
        let mut tokens = Tokens::new();

        let trait_def: Tokens = quote! {
            #[async_trait::async_trait(?Send)]
            pub trait Client {
                type Error;

                $(for o in &self.analysis.operations() =>
                    $(Self::write_operation_function_signature(o));
                )
            }
        };
        tokens.append(&trait_def);
        tokens.line();

        let unexpected_response_error_def: Tokens = quote! {
            #[derive(Debug, Clone)]
            pub struct UnexpectedResponse {
                method: String,
                url: String,
                status_code: u16
            }

            impl std::fmt::Display for UnexpectedResponse {
                fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                    write!(f, "unexpected status code {} from {} {}", self.status_code, self.method, self.url)
                }
            }

            impl std::error::Error for UnexpectedResponse {}
        };
        tokens.append(&unexpected_response_error_def);
        tokens.line();

        tokens.append(&self.write_awc_client());

        Ok(tokens)
    }

    // This function should probably be somewhere else
    pub fn write_operation_function_signature(o: &OperationDef) -> Tokens {
        quote! {
            async fn $(o.name())(
                &self,
                $(for (_, ty) in o.path_params() join (, ) => $(ty.name()): $(ty.schema_type()))$(if o.path_params().count() > 0 { ,  })
                $(if let Some(b) = o.request_body() { body: $(b),  })
            ) -> Result<$(o.response()), Self::Error>
        }
    }

    fn write_awc_client(&self) -> Tokens {
        let mut tokens = Tokens::new();

        let awc_client_def: Tokens = quote! {
            pub struct AwcClient {
                c: awc::Client,
                base_url: String,
            }

            impl AwcClient {
                pub fn new(c: &awc::Client, base_url: &str) -> Self {
                    Self {
                        c: c.clone(),
                        base_url: base_url.to_owned(),
                    }
                }
            }
        };
        tokens.append(&awc_client_def);
        tokens.line();

        let awc_client_impl: Tokens = quote! {
            #[async_trait::async_trait(?Send)]
            impl Client for AwcClient {
                type Error = Box<dyn std::error::Error>;
                
                $(for o in &self.analysis.operations() =>
                    $(Self::write_operation_function_signature(o)) {
                        let method = $(self.write_awc_method(o));
                        let url = $(self.write_awc_path(o));
                        let $(if o.has_any_response_body() { mut }) res = self.c.request(method.clone(), url.clone()).$(if o.request_body().is_some() { send_json(&body) } else { send() }).await?;
                        match res.status().as_u16() {
                            $(for (status_code, r) in o.responses() join (, ) => $(self.write_awc_response_handler(o, status_code, r))),
                            $(if !o.has_default_response() {
                                _ => Err(Box::new(UnexpectedResponse {
                                    method: method.to_string(),
                                    url: url.to_owned(),
                                    status_code: res.status().as_u16()
                                })),
                            })
                        }
                    }
                )
            }
        };
        tokens.append(&awc_client_impl);
        tokens.line();

        tokens
    }

    fn write_awc_method(&self, o: &OperationDef) -> Tokens {
        match *o.method() {
            Method::CONNECT => quote! { awc::http::Method::CONNECT },
            Method::DELETE => quote! { awc::http::Method::DELETE },
            Method::GET => quote! { awc::http::Method::GET },
            Method::HEAD => quote! { awc::http::Method::HEAD },
            Method::OPTIONS => quote! { awc::http::Method::OPTIONS },
            Method::PATCH => quote! { awc::http::Method::PATCH },
            Method::POST => quote! { awc::http::Method::POST },
            Method::PUT => quote! { awc::http::Method::PUT },
            Method::TRACE => quote! { awc::http::Method::TRACE },
            _ => panic!("unknown method `{:?}` for awc client", o.method()),
        }
    }

    fn write_awc_path(&self, o: &OperationDef) -> Tokens {
        let format_string = o.path().iter().fold("{}".to_owned(), |memo, v| match v {
            SegmentOrParameter::Segment(s) => format!("{}/{}", memo, s),
            SegmentOrParameter::Parameter(_) => format!("{}/{{}}", memo),
        });

        let arguments = o
            .path()
            .iter()
            .flat_map(|s| s.as_parameter())
            .flat_map(|p| o.param_by_name(p, ParameterLocation::Path));

        quote! { format!($(quoted(format_string)), self.base_url, $(for a in arguments join (, ) => $(a.name()))) }
    }

    fn write_awc_response_handler(
        &self,
        operation: &OperationDef,
        status_code: &str,
        response: &Option<String>,
    ) -> Tokens {
        let match_value: Tokens = match status_code.parse::<u16>() {
            Ok(status_code) => quote! { $(status_code) },
            Err(_) if status_code == "default" => quote! { _ },
            _ => panic!("could not parse status code {}", &status_code),
        };
        let match_arm: Tokens = if operation.responses().len() == 1 {
            if operation.response() == "()" {
                quote! { Ok($(operation.response())) }
            } else {
                quote! {
                    {
                        let body: $(operation.response()) = res.json().await?;
                        Ok(body)
                    }
                }
            }
        } else {
            match response {
                Some(schema_type) => quote! {
                    {
                        let body: $(schema_type) = res.json().await?;
                        Ok($(operation.response())::S$(status_code)(body))
                    }
                },
                None => quote! {
                    Ok($(operation.response())::S$(status_code))
                },
            }
        };

        quote! { $(match_value) => $(match_arm) }
    }
}
