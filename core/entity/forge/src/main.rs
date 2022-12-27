use std::convert::Infallible;

use aws_sdk_secretsmanager::{Client as SecretsManagerClient, Error, PKG_VERSION, Region};
use macaroon::{Format, Macaroon, MacaroonKey};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use warp::Filter;
use warp::http::{Response, StatusCode};
use warp::reply::{Json, WithStatus};

use assemblylift_core_entity::package::EntityManifest;

#[derive(Serialize, Deserialize)]
struct MintRequest {
    pub id: String,
    pub location: String,
    pub policy_document: String,
}

// FIXME /mint assumes that we always want to mint from the root key (ie `key` is always the root key)
//       but really we should look up the key from the id and use whatever that is
//       |
//       --> or is there a forge per TS?
fn mint_request(req: MintRequest, key: &MacaroonKey) -> WithStatus<Vec<u8>> {
    let mut macaroon = Macaroon::create(
        Some(req.location.clone()),
        key,
        req.id.clone().into(),
    )
    .unwrap();
    macaroon.add_first_party_caveat(req.policy_document.clone().into());
    let out = macaroon.serialize(Format::V2JSON).unwrap();
    warp::reply::with_status(out.into(), StatusCode::OK)
}

/// Each forge has a unique Root Key
async fn get_root_key() -> String {
    let shared_config = aws_config::load_from_env().await;
    let client = SecretsManagerClient::new(&shared_config);
    let value = client
        .get_secret_value()
        .secret_id("test/asml/forge")
        .send()
        .await
        .expect("could not get secret");
    let json: Value =
        serde_json::from_str(&value.secret_string.expect("could not get secret as string"))
            .unwrap();
    json.get("root").unwrap().as_str().unwrap().to_string()
}

#[tokio::main]
async fn main() {
    let key = get_root_key().await;
    let mut s = [0u8; 32];
    for (i, b) in key.as_bytes().into_iter().enumerate() {
        if i < 32 {
            s[i] = *b;
        } else {
            break;
        }
    }

    let key = s.to_owned();
    let mint = warp::post()
        .and(warp::path("mint"))
        .and(warp::body::json())
        .map(move |req| mint_request(req, &key.into()));

    warp::serve(mint).run(([0, 0, 0, 0], 3030)).await;
}
