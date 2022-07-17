use macaroon::Macaroon;
use opa::bundle::Bundle;
use opa::wasm::Opa;

fn main() {
    let file_path: String = std::env::args().collect::<Vec<String>>()[1].clone();


    // TODO the Entity Evaluator needs to:
    //       - Stream the Token from stdin
    //       - Extract the OPA WASM from the Token
    //       - Load the OPA WASM module
    //       - Evaluate the Token Policy
    //       - Optionally cache the result for some (short) time
    //       - IFF Token Policy is OK, allow Data Gateway operations
    let bundle = Bundle::from_file(file_path)
        .expect("could not load OPA bundle from file");
    let mut opa = Opa::new().build_from_bundle(&bundle)
        .expect("could not build OPA bundle");

    println!("available entrypoints:");
    for e in opa.entrypoints() {
        println!("{}", e);
    }
}
