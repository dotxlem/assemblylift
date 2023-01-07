use asml_core::*;
use macaroon::{Macaroon, MacaroonKey, Verifier};
use secretsmanager;
use secretsmanager::structs::*;

#[handler]
async fn main() {
    let event: serde_json::Value =
        serde_json::from_str(&ctx.input).expect("could not parse function input as JSON");

    let identity = event["identitySource"].as_array().unwrap()[0]
        .as_str()
        .unwrap()
        .to_string();
    let macaroon = match Macaroon::deserialize(&identity) {
        Ok(m) => m,
        Err(err) => {
            FunctionContext::log(err.to_string());
            return FunctionContext::success("{\"isAuthorized\":false}".to_string());
        }
    };
    let verifier = Verifier::default();
    let key = get_token_key(
        std::str::from_utf8(macaroon.identifier().0.as_slice())
            .unwrap()
            .to_string(),
    )
    .await
    .unwrap(); // TODO catch error once FunctionContext::error is implemented

    match verifier.verify(
        &macaroon,
        &MacaroonKey::generate(key.as_str().as_bytes()),
        vec![],
    ) {
        Ok(_) => FunctionContext::success("{\"isAuthorized\":true}".to_string()),
        Err(_) => FunctionContext::success("{\"isAuthorized\":false}".to_string()),
    }
}

async fn get_token_key(token_id: String) -> Result<String, String> {
    let secret_prefix = std::env::var("ASML_AUTH_TOKEN_SECRET_PREFIX")
        .unwrap_or("asml/auth".into());
    let mut get_secret_req = GetSecretValueRequest::default();
    get_secret_req.secret_id = format!("{}/{}", &secret_prefix, &token_id);
    match secretsmanager::get_secret_value(get_secret_req).await {
        Ok(res) => Ok(res.secret_string.unwrap()),
        Err(err) => Err(err.to_string()),
    }
}
