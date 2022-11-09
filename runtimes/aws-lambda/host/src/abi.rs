use std::cell::Cell;
use std::error::Error;
use std::io;
use std::io::ErrorKind;

use assemblylift_core::abi::RuntimeAbi;
use assemblylift_core::wasm::{WasmMemory, WasmState};

pub struct LambdaAbi;

impl<S> RuntimeAbi<Vec<u8>, S> for LambdaAbi
where
    S: Clone + Send + Sized + 'static,
{
    fn log(state: &dyn WasmState<Vec<u8>, S>, ptr: u32, len: u32) {
        let string = runtime_ptr_to_string(state, ptr, len).unwrap();
        println!("LOG: {}", string);
    }

    fn success(state: &dyn WasmState<Vec<u8>, S>, ptr: u32, len: u32) {
        let lambda_runtime = &crate::LAMBDA_RUNTIME;
        let response = runtime_ptr_to_string(state, ptr, len).unwrap();

        let respond = lambda_runtime.respond(response.to_string());
        state.threader().spawn(respond);
    }
}

fn runtime_ptr_to_string<S>(state: &dyn WasmState<Vec<u8>, S>, ptr: u32, len: u32) -> Result<String, io::Error>
where
    S: Clone + Send + Sized + 'static,
{
    let str_vec = state.memory_read(ptr as usize, len as usize).unwrap();
    std::str::from_utf8(str_vec.as_slice())
        .map(String::from)
        .map_err(to_io_error)
}

fn to_io_error<E: Error>(err: E) -> io::Error {
    io::Error::new(ErrorKind::Other, err.to_string())
}
