use crate::client_gen::{Client, AwcClient, AddPetResponse, NewPet};

mod client_gen;

#[actix_rt::main]
async fn main() {
    let client = AwcClient::new(&awc::Client::default(), "http://localhost:8080");

    let new_pet = client.add_pet(NewPet { name: "my pet".to_owned(), tag: Some("test".to_owned()) }).await.expect("adding pet should work");
    let pet_id = match &new_pet {
        AddPetResponse::S200(pet) => pet.v1.id,
        r => panic!("unexpected response from add_pet: {:?}", r) 
    };
    println!("new pet: {:?}", &new_pet);

    let result = client.find_pet_by_id(pet_id).await.expect("getting pet should work");
    println!("pet by id: {:?}", result);

    let all_pets = client.find_pets().await.expect("listing pets should work");
    println!("all pets: {:?}", all_pets);

}
