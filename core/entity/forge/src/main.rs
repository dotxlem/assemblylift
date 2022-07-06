use std::convert::Infallible;

use macaroon::{Format, Macaroon, MacaroonKey};
use macaroon::crypto::Encryptor;
use serde::{Deserialize, Serialize};
use warp::Filter;
use warp::http::{Response, StatusCode};
use warp::reply::{Json, WithStatus};

use assemblylift_core_entity::package::EntityManifest;

#[derive(Serialize, Deserialize)]
struct MintRequest {
    pub id: String,
    pub location: String,
}

// TODO sketch out on paper how 1st/3rd party relate to assemblylift
//      there isn't (probably) just one mint request really -- 3rd party discharge is slightly different
//      either way any event 3rd party uses its own request/route
fn mint_request(req: MintRequest) -> WithStatus<Vec<u8>> {
    let key = "dummy-key";
    let mut macaroon = Macaroon::create(
        Some(req.location.clone()),
        &key.into(),
        req.id.clone().into(),
    ).unwrap();
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
