mod abi;

use std::mem::ManuallyDrop;
use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard};

use assemblylift_core::abi::RuntimeAbi;
use wasmer::{imports, Array, ChainableNamedResolver, Cranelift, Function, LazyInit, Memory, Module, NativeFunc, Store, Universal, WasmPtr, WasmerEnv, WasmCell};
use wasmer_wasi::WasiState;

use assemblylift_core::buffers::FunctionInputBuffer;
use assemblylift_core::threader::Threader;
use assemblylift_core::wasm::{Resolver, WasmModule, WasmState};
use assemblylift_core_iomod::registry::RegistryTx;

pub struct Wasmer<R, S>
where
    R: RuntimeAbi<S> + 'static,
    S: Clone + Send + Sized + 'static,
{
    module: Module,
    store: Store,
    resolver: Option<Resolver>,
    state: Option<State<S>>,
    _phantom: std::marker::PhantomData<R>,
}

impl<R, S> WasmModule<S> for Wasmer<R, S>
where
    R: RuntimeAbi<S> + 'static,
    S: Clone + Send + Sized + 'static,
{
    fn deserialize_from_path<P: AsRef<Path>>(path: P) -> anyhow::Result<Self> {
        let compiler = Cranelift::default();
        let store = Store::new(&Universal::new(compiler).engine());
        let module = unsafe { Module::deserialize_from_file(&store, path) };
        match module {
            Ok(module) => Ok(Self {
                module,
                store,
                resolver: None,
                state: None,
                _phantom: Default::default(),
            }),
            Err(e) => Err(anyhow::Error::new(e)),
        }
    }

    fn deserialize_from_bytes<B: AsRef<[u8]>>(bytes: B) -> anyhow::Result<Self> {
        let compiler = Cranelift::default();
        let store = Store::new(&Universal::new(compiler).engine());
        let module = unsafe { Module::deserialize(&store, bytes.as_ref()) };
        match module {
            Ok(module) => Ok(Self {
                module,
                store,
                resolver: None,
                state: None,
                _phantom: Default::default(),
            }),
            Err(e) => Err(anyhow::Error::new(e)),
        }
    }

    fn build(
        &mut self,
        registry_tx: RegistryTx,
        status_tx: crossbeam_channel::Sender<S>,
    ) -> anyhow::Result<()> {
        let wasm_state = State::<S>::new(registry_tx, status_tx);
        let function_env = std::env::var("ASML_FUNCTION_ENV").unwrap_or("default".into());
        let mut wasi_env = match function_env.as_str() {
            "ruby-docker" => WasiState::new("assemblylift-guest")
                .arg("/src/handler.rb")
                .env("RUBY_PLATFORM", "wasm32-wasi")
                .map_dir("/src", "/usr/bin/ruby-wasm32-wasi/src")
                .expect("could not preopen `src` directory")
                .map_dir("/usr", "/usr/bin/ruby-wasm32-wasi/usr")
                .expect("could not map ruby fs")
                // .map_dir("/tmp", "/tmp/asmltmp")
                // .expect("could not map tmpfs")
                .finalize()
                .expect("could not init WASI env"),
            "ruby-lambda" => WasiState::new("assemblylift-guest")
                .arg("/src/handler.rb")
                .env("RUBY_PLATFORM", "wasm32-wasi")
                .map_dir("/src", "/tmp/rubysrc")
                .expect("could not preopen `src` directory")
                .map_dir("/usr", "/tmp/rubyusr")
                .expect("could not map ruby fs")
                .map_dir("/tmp", "/tmp/asmltmp")
                .expect("could not map tmpfs")
                .finalize()
                .expect("could not init WASI env"),
            _ => WasiState::new("assemblylift-guest")
                .map_dir("/tmp", "/tmp/asmltmp")
                .expect("could not map tmpfs")
                .finalize()
                .expect("could not init WASI env"),
        };

        let wasi_imports = wasi_env
            .import_object(&self.module)
            .expect("could not get WASI import object");
        let asml_imports = imports! {
            "env" => {
                "__asml_abi_runtime_log" => Function::new_native_with_env(&self.store, wasm_state.clone(), R::log),
                "__asml_abi_runtime_success" => Function::new_native_with_env(&self.store, wasm_state.clone(), R::success),

                "__asml_abi_invoke" => Function::new_native_with_env(&self.store, wasm_state.clone(), crate::abi::asml_abi_io_invoke), // TODO deprecated, IOmod guests need to update
                "__asml_abi_io_invoke" => Function::new_native_with_env(&self.store, wasm_state.clone(), crate::abi::asml_abi_io_invoke),
                "__asml_abi_io_poll" => Function::new_native_with_env(&self.store, wasm_state.clone(), crate::abi::asml_abi_io_poll),
                "__asml_abi_io_len" => Function::new_native_with_env(&self.store, wasm_state.clone(), crate::abi::asml_abi_io_len),
                "__asml_abi_io_load" => Function::new_native_with_env(&self.store, wasm_state.clone(), crate::abi::asml_abi_io_load),
                "__asml_abi_io_next" => Function::new_native_with_env(&self.store, wasm_state.clone(), crate::abi::asml_abi_io_next),

                "__asml_abi_clock_time_get" => Function::new_native_with_env(&self.store, wasm_state.clone(), crate::abi::asml_abi_clock_time_get),

                "__asml_abi_input_start" => Function::new_native_with_env(&self.store, wasm_state.clone(), crate::abi::asml_abi_input_start),
                "__asml_abi_input_next" => Function::new_native_with_env(&self.store, wasm_state.clone(), crate::abi::asml_abi_input_next),
                "__asml_abi_input_length_get" => Function::new_native_with_env(&self.store, wasm_state.clone(), crate::abi::asml_abi_input_length_get),
            },
        };

        self.resolver = Some(asml_imports.chain_back(wasi_imports));
        Ok(())
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

impl<S> State<S>
where
    S: Clone + Send + Sized + 'static,
{
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

    fn memory_read<B: AsRef<[u8]>>(&self, offset: usize, length: usize) -> anyhow::Result<B> {
        todo!()
    }

    fn memory_write<B: AsRef<[u8]>>(&self, offset: usize, bytes: B) -> anyhow::Result<usize> {
        let wasm_memory = self.memory_ref().unwrap();
        let input_buffer = env
            .get_function_input_buffer
            .get_ref()
            .unwrap()
            .call()
            .unwrap();
        let memory_writer: Vec<WasmCell<u8>> = input_buffer
            .deref(&wasm_memory, offset as u32, bytes.len() as u32)
            .unwrap();

        let mut bytes_out = 0usize;
        for (i, b) in bytes.iter().enumerate() {
            let idx = i + dst.0;
            memory_writer[idx].set(*b);
            bytes_out += 1;
        }
        Ok(bytes_out)
    }
}
