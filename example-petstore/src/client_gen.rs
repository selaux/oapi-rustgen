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

#[async_trait::async_trait(?Send)]
pub trait Client {
    type Error;
    async fn find_pets(&self) -> Result<FindPetsResponse, Self::Error>;
    async fn add_pet(&self, body: NewPet) -> Result<AddPetResponse, Self::Error>;
    async fn find_pet_by_id(&self, id: i64) -> Result<FindPetByIdResponse, Self::Error>;
    async fn delete_pet(&self, id: i64) -> Result<DeletePetResponse, Self::Error>;
}

#[derive(Debug, Clone)]
pub struct UnexpectedResponse {
    method: String,
    url: String,
    status_code: u16,
}
impl std::fmt::Display for UnexpectedResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "unexpected status code {} from {} {}",
            self.status_code, self.method, self.url
        )
    }
}
impl std::error::Error for UnexpectedResponse {}

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

#[async_trait::async_trait(?Send)]
impl Client for AwcClient {
    type Error = Box<dyn std::error::Error>;
    async fn find_pets(&self) -> Result<FindPetsResponse, Self::Error> {
        let method = awc::http::Method::GET;
        let url = format!("{}/pets", self.base_url,);
        let mut res = self.c.request(method.clone(), url.clone()).send().await?;
        match res.status().as_u16() {
            200 => {
                let body: Vec<Pet> = res.json().await?;
                Ok(FindPetsResponse::S200(body))
            }
            _ => {
                let body: Error = res.json().await?;
                Ok(FindPetsResponse::Sdefault(body))
            }
        }
    }
    async fn add_pet(&self, body: NewPet) -> Result<AddPetResponse, Self::Error> {
        let method = awc::http::Method::POST;
        let url = format!("{}/pets", self.base_url,);
        let mut res = self
            .c
            .request(method.clone(), url.clone())
            .send_json(&body)
            .await?;
        match res.status().as_u16() {
            200 => {
                let body: Pet = res.json().await?;
                Ok(AddPetResponse::S200(body))
            }
            _ => {
                let body: Error = res.json().await?;
                Ok(AddPetResponse::Sdefault(body))
            }
        }
    }
    async fn find_pet_by_id(&self, id: i64) -> Result<FindPetByIdResponse, Self::Error> {
        let method = awc::http::Method::GET;
        let url = format!("{}/pets/{}", self.base_url, id);
        let mut res = self.c.request(method.clone(), url.clone()).send().await?;
        match res.status().as_u16() {
            200 => {
                let body: Pet = res.json().await?;
                Ok(FindPetByIdResponse::S200(body))
            }
            _ => {
                let body: Error = res.json().await?;
                Ok(FindPetByIdResponse::Sdefault(body))
            }
        }
    }
    async fn delete_pet(&self, id: i64) -> Result<DeletePetResponse, Self::Error> {
        let method = awc::http::Method::DELETE;
        let url = format!("{}/pets/{}", self.base_url, id);
        let mut res = self.c.request(method.clone(), url.clone()).send().await?;
        match res.status().as_u16() {
            204 => Ok(DeletePetResponse::S204),
            _ => {
                let body: Error = res.json().await?;
                Ok(DeletePetResponse::Sdefault(body))
            }
        }
    }
}
