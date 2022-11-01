use std::mem::ManuallyDrop;
use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard};

use wasmer::{
    Array, Cranelift, LazyInit, Memory, Module, NativeFunc, Store, Universal, WasmPtr, WasmerEnv,
};

use assemblylift_core::buffers::FunctionInputBuffer;
use assemblylift_core::threader::Threader;
use assemblylift_core::wasm::{WasmModule, WasmState};

pub struct Wasmer {
    module: Module,
    store: Store,
}

impl WasmModule for Wasmer {
    fn deserialize_from_path<P: AsRef<Path>>(path: P) -> anyhow::Result<Self> {
        let compiler = Cranelift::default();
        let store = Store::new(&Universal::new(compiler).engine());
        let module = unsafe { Module::deserialize_from_file(&store, path) };
        match module {
            Ok(module) => Ok(Self { module, store }),
            Err(e) => Err(anyhow::Error::new(e)),
        }
    }

    fn deserialize_from_bytes<B: AsRef<[u8]>>(bytes: B) -> anyhow::Result<Self> {
        let compiler = Cranelift::default();
        let store = Store::new(&Universal::new(compiler).engine());
        let module = unsafe { Module::deserialize(&store, bytes.as_ref()) };
        match module {
            Ok(module) => Ok(Self { module, store }),
            Err(e) => Err(anyhow::Error::new(e)),
        }
    }

    fn build<S: Clone + Send + Sized + 'static>(
        &self,
        registry_tx: RegistryTx,
        status_tx: crossbeam_channel::Sender<S>,
    ) -> anyhow::Result<()> {
        let wasm_state = State::<S>::new(registry_tx, status_tx);
    }

    fn instantiate(&self) -> anyhow::Result<()> {
        todo!()
    }
}

#[derive(WasmerEnv, Clone)]
pub struct State<S>
where
    S: Clone + Send + Sized + 'static,
{
    threader: ManuallyDrop<Arc<Mutex<Threader<S>>>>,
    host_input_buffer: Arc<Mutex<FunctionInputBuffer>>,
    #[wasmer(export)]
    memory: LazyInit<Memory>,
    #[wasmer(export(name = "__asml_guest_get_function_input_buffer_pointer"))]
    get_function_input_buffer: LazyInit<NativeFunc<(), WasmPtr<u8, Array>>>,
    #[wasmer(export(name = "__asml_guest_get_io_buffer_pointer"))]
    get_io_buffer: LazyInit<NativeFunc<(), WasmPtr<u8, Array>>>,
    status_sender: crossbeam_channel::Sender<S>,
}

impl<S> State<S> {
    pub fn new(registry_tx: RegistryTx, status_tx: crossbeam_channel::Sender<S>) -> Self {
        Self {
            threader: ManuallyDrop::new(Arc::new(Mutex::new(Threader::new(registry_tx)))),
            host_input_buffer: Arc::new(Mutex::new(FunctionInputBuffer::new())),
            memory: Default::default(),
            get_function_input_buffer: Default::default(),
            get_io_buffer: Default::default(),
            status_sender: status_tx,
        }
    }
}

impl<S> WasmState<S> for State<S>
where
    S: Clone + Send + Sized + 'static,
{
    fn threader(&self) -> MutexGuard<Threader<S>> {
        self.threader.lock().unwrap()
    }
}
