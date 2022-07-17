use macaroon::Macaroon;
use wasmer::{Cranelift, Module, Store, Universal};

fn main() {
    let module_name = "";
    let module_path = "";
    let file_path = format!("{}/{}", module_path, module_name);

    let compiler = Cranelift::default();
    let store = Store::new(&Universal::new(compiler).engine());
    let module = unsafe { Module::deserialize_from_file(&store, file_path.clone()) }
            .expect(&format!("could not load wasm from {}", file_path.clone()));

    // TODO the Entity Evaluator needs to:
    //       - Load the OPA WASM module
    //       - Stream the Token from stdin
    //       - Load the Token Policy into the OPA WASM
    //       - Evaluate the Token Policy
    //       - Optionally cache the result for some (short) time
    //       - IFF Token Policy is OK, allow Data Gateway operations
}
