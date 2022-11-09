use crate::wasm::WasmState;

pub trait RuntimeAbi<B: AsRef<[u8]>, S: Clone + Send + Sized + 'static> {
    fn log(env: &dyn WasmState<B, S>, ptr: u32, len: u32);
    fn success(env: &dyn WasmState<B, S>, ptr: u32, len: u32);
}
