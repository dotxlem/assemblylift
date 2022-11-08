use std::cell::Cell;
use std::error::Error;
use std::io;
use std::io::ErrorKind;
use std::time::{SystemTime, UNIX_EPOCH};

use wasmer::{Array, MemoryView, WasmPtr};

use crate::buffers::{LinearBuffer, PagedWasmBuffer};
use crate::wasm::WasmState;

pub trait RuntimeAbi<S: Clone + Send + Sized + 'static> {
    fn log(env: &dyn WasmState<Vec<u8>, S>, ptr: u32, len: u32);
    fn success(env: &dyn WasmState<Vec<u8>, S>, ptr: u32, len: u32);
}
