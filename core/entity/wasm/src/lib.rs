use macaroon::Macaroon;

extern "C" {
    // TODO entity ABI
}

fn test() {
    let m = Macaroon::create(Some("asml/service/test-svc".into()), &key, id);
}