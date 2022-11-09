use wasmer::{Array, WasmPtr};
use crate::wasm::WasmState;

pub type WasmBufferPtr = WasmPtr<u8, Array>;

pub mod abi;
pub mod buffers;
pub mod threader;
pub mod wasm;

#[inline(always)]
/// Invoke an IOmod call at coordinates `method_path` with input `method_input`
pub fn invoke_io<S>(state: &dyn WasmState<Vec<u8>, S>, method_path: &str, method_input: Vec<u8>) -> i32
where
    S: Clone + Send + Sized + 'static,
{
    let ioid = state
        .threader()
        .next_ioid()
        .expect("unable to get a new IO ID");

    state
        .threader()
        .invoke(method_path, method_input, ioid);

    ioid as i32
}
