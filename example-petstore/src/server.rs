use std::{
    future::ready,
    pin::Pin,
    sync::{Arc, Mutex},
};

use crate::server_gen::{handler, Handlers, NewPet, PetV1};
use actix_web::{middleware, web, App, FromRequest, HttpServer};
use futures::{Future, TryFutureExt};
use server_gen::{FindPetsResponse, Pet};

mod server_gen;

#[derive(Debug)]
struct PetDatabase {
    next_incement: Arc<Mutex<i64>>,
    pets: Arc<Mutex<Vec<Pet>>>,
}

impl Default for PetDatabase {
    fn default() -> Self {
        Self {
            next_incement: Arc::new(Mutex::new(1)),
            pets: Arc::new(Mutex::new(vec![])),
        }
    }
}
struct Impl {
    db: Arc<PetDatabase>,
}

impl FromRequest for Impl {
    type Error = actix_web::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self, Self::Error>>>>;

    fn from_request(
        req: &actix_web::HttpRequest,
        payload: &mut actix_web::dev::Payload,
    ) -> Self::Future {
        Box::pin(
            web::Data::from_request(req, payload)
                .and_then(|d: web::Data<PetDatabase>| ready(Ok(Self { db: (*d).clone() }))),
        )
    }
}

#[async_trait::async_trait(?Send)]
impl Handlers for Impl {
    type Error = actix_web::Error;

    async fn find_pets(&self) -> Result<server_gen::FindPetsResponse, Self::Error> {
        let pets = self.db.pets.lock().expect("lock should not be poisoned");

        Ok(FindPetsResponse::S200(pets.clone()))
    }
    async fn add_pet(
        &self,
        body: server_gen::NewPet,
    ) -> Result<server_gen::AddPetResponse, Self::Error> {
        let mut inc = self
            .db
            .next_incement
            .lock()
            .expect("lock should not be poisoned");
        let mut pets = self.db.pets.lock().expect("lock should not be poisoned");
        let pet = Pet {
            v0: NewPet {
                name: body.name,
                tag: body.tag,
            },
            v1: PetV1 { id: *inc },
        };

        *inc += 1;
        pets.push(pet.clone());

        Ok(server_gen::AddPetResponse::S200(pet))
    }
    async fn find_pet_by_id(
        &self,
        id: i64,
    ) -> Result<server_gen::FindPetByIdResponse, Self::Error> {
        let pets = self.db.pets.lock().expect("lock should not be poisoned");

        match pets.iter().find(|p| p.v1.id == id) {
            Some(pet) => Ok(server_gen::FindPetByIdResponse::S200(pet.clone())),
            None => Ok(server_gen::FindPetByIdResponse::Sdefault(
                server_gen::Error {
                    code: 404,
                    message: "not found".to_owned(),
                },
            )),
        }
    }
    async fn delete_pet(&self, id: i64) -> Result<server_gen::DeletePetResponse, Self::Error> {
        let mut pets = self.db.pets.lock().expect("lock should not be poisoned");
        pets.retain(|p| p.v1.id != id);
        Ok(server_gen::DeletePetResponse::S204)
    }
}

#[actix_rt::main]
async fn main() -> std::io::Result<()> {
    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(PetDatabase::default()))
            .wrap(middleware::Logger::default())
            .default_service(web::to(handler::<Impl, actix_web::Error>))
    })
    .bind(("0.0.0.0", 8080))?
    .workers(2)
    .run()
    .await
}
