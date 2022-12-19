//! AssemblyLift WASM Buffers
//! See [core-buffers doc](../../docs/core-buffers.md) for more details

use std::collections::HashMap;

use tokio::sync::mpsc;

use assemblylift_core_io_common::constants::{FUNCTION_INPUT_BUFFER_SIZE, IO_BUFFER_SIZE_BYTES};

use crate::wasm::MemoryMessage;

/// A trait representing a linear byte buffer, such as Vec<u8>
pub trait LinearBuffer {
    /// Initialize the buffer with the contents of `buffer`
    fn initialize(&mut self, buffer: Vec<u8>);
    /// Write bytes to the buffer at an offset
    fn write(&mut self, bytes: &[u8], at_offset: usize) -> usize;
    /// Erase `len` bytes starting from `offset`
    fn erase(&mut self, offset: usize, len: usize) -> usize;
    /// The length of the buffer in bytes
    fn len(&self) -> usize;
    /// The capacity of the buffer in bytes
    fn capacity(&self) -> usize;
}

/// A trait representing a buffer in WASM guest memory
pub trait WasmBuffer {
    fn copy_to_wasm(
        &self,
        memory_writer: mpsc::Sender<MemoryMessage>,
        src: (usize, usize),
        dst: (usize, usize),
    ) -> Result<(), ()>;
}

/// Implement paging data into a `WasmBuffer`
pub trait PagedWasmBuffer: WasmBuffer {
    fn first(&mut self, memory_writer: mpsc::Sender<MemoryMessage>, offset: Option<Vec<usize>>) -> i32;
    fn next(&mut self, memory_writer: mpsc::Sender<MemoryMessage>, offset: Option<Vec<usize>>) -> i32;
}

pub struct FunctionInputBuffer {
    buffer: Vec<u8>,
    page_idx: usize,
}

impl FunctionInputBuffer {
    pub fn new() -> Self {
        Self {
            buffer: Vec::new(),
            page_idx: 0usize,
        }
    }
}

impl LinearBuffer for FunctionInputBuffer {
    fn initialize(&mut self, buffer: Vec<u8>) {
        self.buffer = buffer;
    }

    fn write(&mut self, bytes: &[u8], at_offset: usize) -> usize {
        let mut bytes_written = 0usize;
        for idx in at_offset..bytes.len() {
            self.buffer[idx] = bytes[idx - at_offset];
            bytes_written += 1;
        }
        bytes_written
    }

    fn erase(&mut self, offset: usize, len: usize) -> usize {
        let mut bytes_erased = 0usize;
        for idx in offset..len {
            self.buffer[idx] = 0;
            bytes_erased += 1;
        }
        bytes_erased
    }

    fn len(&self) -> usize {
        self.buffer.len()
    }

    fn capacity(&self) -> usize {
        self.buffer.capacity()
    }
}

impl PagedWasmBuffer for FunctionInputBuffer {
    fn first(&mut self, memory_writer: mpsc::Sender<MemoryMessage>, offset: Option<Vec<usize>>) -> i32 {
        let end: usize = match self.buffer.len() < FUNCTION_INPUT_BUFFER_SIZE {
            true => self.buffer.len(),
            false => FUNCTION_INPUT_BUFFER_SIZE,
        };
        self.copy_to_wasm(memory_writer, (0usize, end), (offset.unwrap()[0], FUNCTION_INPUT_BUFFER_SIZE))
            .unwrap();
        self.page_idx = 0usize;
        0
    }

    fn next(&mut self, memory_writer: mpsc::Sender<MemoryMessage>, offset: Option<Vec<usize>>) -> i32 {
        use std::cmp::min;
        if self.buffer.len() > FUNCTION_INPUT_BUFFER_SIZE {
            self.page_idx += 1;
            self.copy_to_wasm(
                memory_writer,
                (
                    FUNCTION_INPUT_BUFFER_SIZE * self.page_idx,
                    min(
                        FUNCTION_INPUT_BUFFER_SIZE * (self.page_idx + 1),
                        self.buffer.len(),
                    ),
                ),
                (offset.unwrap()[0], FUNCTION_INPUT_BUFFER_SIZE),
            )
            .unwrap();
        }
        0
    }
}

impl WasmBuffer for FunctionInputBuffer {
    fn copy_to_wasm(
        &self,
        memory_writer: mpsc::Sender<MemoryMessage>,
        src: (usize, usize),
        dst: (usize, usize),
    ) -> Result<(), ()> {
        for (i, b) in self.buffer[src.0..src.1].iter().enumerate() {
            let idx = i + dst.0;
            memory_writer.blocking_send((idx, *b)).unwrap();
        }

        Ok(())
    }
}

pub struct IoBuffer {
    active_buffer: usize,
    buffers: HashMap<usize, Vec<u8>>,
    page_indices: HashMap<usize, usize>,
}

impl IoBuffer {
    pub fn new() -> Self {
        Self {
            active_buffer: 0usize,
            buffers: Default::default(),
            page_indices: Default::default(),
        }
    }

    pub fn len(&self, ioid: usize) -> usize {
        self.buffers.get(&ioid).unwrap().len()
    }

    pub fn with_capacity(num_buffers: usize, buffer_capacity: usize) -> Self {
        let mut buffers: HashMap<usize, Vec<u8>> = HashMap::new();
        let mut indices: HashMap<usize, usize> = HashMap::new();
        for idx in 0..num_buffers {
            buffers.insert(idx, Vec::with_capacity(buffer_capacity));
            indices.insert(idx, 0);
        }
        Self {
            active_buffer: 0usize,
            buffers,
            page_indices: indices,
        }
    }

    pub fn write(&mut self, ioid: usize, bytes: &[u8]) -> usize {
        let mut bytes_written = 0usize;
        match self.buffers.get_mut(&ioid) {
            Some(buffer) => {
                for idx in 0..bytes.len() {
                    buffer.push(bytes[idx]);
                    bytes_written += 1;
                }
            }
            None => {
                self.buffers.insert(ioid, Vec::new());
                return self.write(ioid, bytes);
            }
        }
        bytes_written
    }
}

impl PagedWasmBuffer for IoBuffer {
    fn first(&mut self, memory_writer: mpsc::Sender<MemoryMessage>, offset: Option<Vec<usize>>) -> i32 {
        match offset {
            Some(offset) => {
                self.active_buffer = offset[0];
                self.page_indices.insert(self.active_buffer, 0usize);

                self.copy_to_wasm(
                    memory_writer,
                    (self.active_buffer, 0usize),
                    (offset[1], IO_BUFFER_SIZE_BYTES),
                )
                    .unwrap();
                0
            }
            None => -1,
        }
    }

    fn next(&mut self, memory_writer: mpsc::Sender<MemoryMessage>, offset: Option<Vec<usize>>) -> i32 {
        let page_idx = self.page_indices.get(&self.active_buffer).unwrap() + 1;
        let page_offset = page_idx * IO_BUFFER_SIZE_BYTES;
        self.copy_to_wasm(
            memory_writer,
            (self.active_buffer, page_offset),
            (offset.unwrap()[0], IO_BUFFER_SIZE_BYTES),
        )
        .unwrap();
        *self.page_indices.get_mut(&self.active_buffer).unwrap() = page_idx;
        0
    }
}

impl WasmBuffer for IoBuffer {
    fn copy_to_wasm(
        &self,
        memory_writer: mpsc::Sender<MemoryMessage>,
        src: (usize, usize),
        dst: (usize, usize),
    ) -> Result<(), ()> {
        use std::cmp::min;
        let buffer = self.buffers.get(&src.0).unwrap();
        for (i, b) in buffer[src.1..min(src.1 + IO_BUFFER_SIZE_BYTES, buffer.len())]
            .iter()
            .enumerate()
        {
            let idx = i + dst.0;
            memory_writer.blocking_send((idx, *b)).unwrap();
        }

        Ok(())
    }
}
