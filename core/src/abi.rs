use crate::wasm::WasmState;

pub trait RuntimeAbi<S: Clone + Send + Sized + 'static> {
    fn log(env: &dyn WasmState<Vec<u8>, S>, ptr: u32, len: u32);
    fn success(env: &dyn WasmState<Vec<u8>, S>, ptr: u32, len: u32);
}
