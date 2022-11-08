use std::cell::Cell;
use std::error::Error;
use std::io;
use std::io::ErrorKind;
use std::time::{SystemTime, UNIX_EPOCH};

use wasmer::MemoryView;

use assemblylift_core::buffers::{LinearBuffer, PagedWasmBuffer};
use assemblylift_core::wasm::WasmState;

use crate::State;

pub fn asml_abi_io_invoke<S>(
    state: &State<S>,
    name_ptr: u32,
    name_len: u32,
    input: u32,
    input_len: u32,
) -> i32
where
    S: Clone + Send + Sized + 'static,
{
    if let Ok(method_path) = env_ptr_to_string(state, name_ptr, name_len) {
        if let Ok(input) = ptr_to_bytes(state, input, input_len) {
            return invoke_io(state, &*method_path, input);
        }
    }

    -1i32 // error
}

pub fn asml_abi_io_poll<S>(state: &State<S>, id: u32) -> i32
where
    S: Clone + Send + Sized + 'static,
{
    state.threader().poll(id) as i32
}

pub fn asml_abi_io_len<S>(state: &State<S>, id: u32) -> u32
where
    S: Clone + Send + Sized + 'static,
{
    state.threader()
        .get_io_memory_document(id)
        .unwrap()
        .length as u32
}

pub fn asml_abi_io_load<S>(state: &State<S>, id: u32) -> i32
where
    S: Clone + Send + Sized + 'static,
{
    match state.threader().document_load(state, id) {
        Ok(_) => 0,
        Err(_) => -1,
    }
}

pub fn asml_abi_io_next<S>(state: &State<S>) -> i32
where
    S: Clone + Send + Sized + 'static,
{
    match state.threader().document_next(state) {
        Ok(_) => 0,
        Err(_) => -1,
    }
}

pub fn asml_abi_clock_time_get<S>(_env: &State<S>) -> u64
where
    S: Clone + Send + Sized + 'static,
{
    let start = SystemTime::now();
    let unix_time = start.duration_since(UNIX_EPOCH).expect("time is broken");
    unix_time.as_secs() * 1000u64
}

pub fn asml_abi_input_start<S>(state: &State<S>) -> i32
where
    S: Clone + Send + Sized + 'static,
{
    state.host_input_buffer
        .clone()
        .lock()
        .unwrap()
        .first(state, None)
}

pub fn asml_abi_input_next<S>(state: &State<S>) -> i32
where
    S: Clone + Send + Sized + 'static,
{
    state.host_input_buffer.clone().lock().unwrap().next(state)
}

pub fn asml_abi_input_length_get<S>(state: &State<S>) -> u64
where
    S: Clone + Send + Sized + 'static,
{
    state.host_input_buffer.clone().lock().unwrap().len() as u64
}

// --- //

#[inline(always)]
/// Invoke an IOmod call at coordinates `method_path` with input `method_input`
fn invoke_io<S>(state: &State<S>, method_path: &str, method_input: Vec<u8>) -> i32
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

fn env_ptr_to_string<S>(state: &State<S>, ptr: u32, len: u32) -> Result<String, io::Error>
where
    S: Clone + Send + Sized + 'static,
{
    let mem = state.memory_ref().unwrap();
    let view: MemoryView<u8> = mem.view();

    let mut str_vec: Vec<u8> = Vec::new();
    for byte in view[ptr as usize..(ptr + len) as usize]
        .iter()
        .map(Cell::get)
    {
        str_vec.push(byte);
    }

    std::str::from_utf8(str_vec.as_slice())
        .map(String::from)
        .map_err(to_io_error)
}

fn ptr_to_bytes<S>(state: &State<S>, ptr: u32, len: u32) -> Result<Vec<u8>, io::Error>
where
    S: Clone + Send + Sized + 'static,
{
    let mem = state.memory_ref().unwrap();
    let view: MemoryView<u8> = mem.view();

    let mut bytes: Vec<u8> = Vec::new();
    for byte in view[ptr as usize..(ptr + len) as usize]
        .iter()
        .map(Cell::get)
    {
        bytes.push(byte);
    }

    Ok(bytes)
}

fn to_io_error<E: Error>(err: E) -> io::Error {
    io::Error::new(ErrorKind::Other, err.to_string())
}
