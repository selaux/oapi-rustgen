#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Pet {
    #[serde(flatten)]
    pub v0: NewPet,
    #[serde(flatten)]
    pub v1: PetV1,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PetV1 {
    pub id: i64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct NewPet {
    pub name: String,
    pub tag: Option<String>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Error {
    pub code: i32,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum FindPetsResponse {
    S200(Vec<Pet>),
    Sdefault(Error),
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum AddPetResponse {
    S200(Pet),
    Sdefault(Error),
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum FindPetByIdResponse {
    S200(Pet),
    Sdefault(Error),
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum DeletePetResponse {
    S204,
    Sdefault(Error),
}

use futures::StreamExt;
use std::str::FromStr;

#[async_trait::async_trait(?Send)]
pub trait Handlers {
    type Error: std::fmt::Debug;
    async fn find_pets(&self) -> Result<FindPetsResponse, Self::Error>;
    async fn add_pet(&self, body: NewPet) -> Result<AddPetResponse, Self::Error>;
    async fn find_pet_by_id(&self, id: i64) -> Result<FindPetByIdResponse, Self::Error>;
    async fn delete_pet(&self, id: i64) -> Result<DeletePetResponse, Self::Error>;
}

pub async fn handler<T, E>(
    req: actix_web::HttpRequest,
    mut payload: actix_web::web::Payload,
) -> Result<actix_web::HttpResponse, actix_web::Error>
where
    T: Handlers + actix_web::FromRequest<Error = E>,
    E: std::fmt::Debug,
{
    let handlers = T::extract(&req).await.expect("handler data should be set");
    let method = req.method();
    let path: Vec<_> = req.path().split('/').skip(1).collect();
    let mut body = actix_web::web::BytesMut::new();
    while let Some(item) = payload.next().await {
        body.extend_from_slice(&item.expect("should read"));
    }
    if let &["pets"] = path.as_slice() {
        if method == actix_web::http::Method::GET {
            let response = handlers.find_pets().await.expect("should execute");
            match response {
                FindPetsResponse::S200(body) => {
                    return Ok(actix_web::HttpResponseBuilder::new(
                        actix_web::http::StatusCode::from_u16(200).expect("valid status code"),
                    )
                    .json(body));
                }
                FindPetsResponse::Sdefault(body) => {
                    return Ok(actix_web::HttpResponseBuilder::new(
                        actix_web::http::StatusCode::from_u16(500).expect("valid status code"),
                    )
                    .json(body));
                }
            }
        }
        if method == actix_web::http::Method::POST {
            let body: NewPet = serde_json::from_slice(&body).expect("body should deserialize");
            let response = handlers.add_pet(body).await.expect("should execute");
            match response {
                AddPetResponse::S200(body) => {
                    return Ok(actix_web::HttpResponseBuilder::new(
                        actix_web::http::StatusCode::from_u16(200).expect("valid status code"),
                    )
                    .json(body));
                }
                AddPetResponse::Sdefault(body) => {
                    return Ok(actix_web::HttpResponseBuilder::new(
                        actix_web::http::StatusCode::from_u16(500).expect("valid status code"),
                    )
                    .json(body));
                }
            }
        }
    }
    if let &["pets", id] = path.as_slice() {
        if method == actix_web::http::Method::GET {
            let id = i64::from_str(id).expect("should deserialize");
            let response = handlers.find_pet_by_id(id).await.expect("should execute");
            match response {
                FindPetByIdResponse::S200(body) => {
                    return Ok(actix_web::HttpResponseBuilder::new(
                        actix_web::http::StatusCode::from_u16(200).expect("valid status code"),
                    )
                    .json(body));
                }
                FindPetByIdResponse::Sdefault(body) => {
                    return Ok(actix_web::HttpResponseBuilder::new(
                        actix_web::http::StatusCode::from_u16(500).expect("valid status code"),
                    )
                    .json(body));
                }
            }
        }
        if method == actix_web::http::Method::DELETE {
            let id = i64::from_str(id).expect("should deserialize");
            let response = handlers.delete_pet(id).await.expect("should execute");
            match response {
                DeletePetResponse::S204 => {
                    return Ok(actix_web::HttpResponseBuilder::new(
                        actix_web::http::StatusCode::from_u16(204).expect("valid status code"),
                    )
                    .finish());
                }
                DeletePetResponse::Sdefault(body) => {
                    return Ok(actix_web::HttpResponseBuilder::new(
                        actix_web::http::StatusCode::from_u16(500).expect("valid status code"),
                    )
                    .json(body));
                }
            }
        }
    }
    todo!();
}
