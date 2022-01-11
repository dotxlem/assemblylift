use std::convert::Infallible;
use macaroon::{Format, Macaroon, MacaroonKey};
use macaroon::crypto::Encryptor;
use serde::{Deserialize, Serialize};
use warp::Filter;
use warp::http::{Response, StatusCode};
use warp::reply::{Json, WithStatus};

use assemblylift_core_entity::package::EntityManifest;

struct VaultEncryptor;

impl Encryptor for VaultEncryptor {
    fn encrypt(with_key: MacaroonKey, clear_bytes: &[u8]) -> macaroon::Result<Vec<u8>> {
        todo!()
    }
}

#[derive(Serialize, Deserialize)]
struct MintRequest {
    pub id: String,
    pub location: String,
}

fn mint_request(req: MintRequest) -> WithStatus<Vec<u8>> {
    let key = "dummy-key";
    let mut macaroon = Macaroon::create(
        Some(req.location.clone()),
        &key.into(),
        req.id.clone().into(),
    ).unwrap();
    // TODO need to look keys up by their ID; where does this happen?
    //      vault needs the ID, but also want to support anonymous literal
    macaroon.add_third_party_caveat::<VaultEncryptor>(loc, key, id);
    let out = macaroon.serialize(Format::V2JSON).unwrap();
    warp::reply::with_status(out, StatusCode::OK)
}

#[tokio::main]
async fn main() {
    let mint = warp::post()
        .and(warp::path("mint"))
        .and(warp::body::json())
        .map(mint_request);

    warp::serve(mint)
        .run(([100, 66, 60, 79], 3030))
        .await;
}
