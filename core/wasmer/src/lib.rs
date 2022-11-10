use std::cell::Cell;
use std::io::Write;
use std::mem::ManuallyDrop;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::str::FromStr;
use std::sync::{Arc, Mutex, MutexGuard};

use itertools::Itertools;
use wasmer::{Array, ChainableNamedResolver, CpuFeature, Cranelift, Function, ImportObject, imports, Instance, LazyInit, Memory, MemoryView, Module, NamedResolverChain, NativeFunc, Store, Target, Triple, Universal, WasmerEnv, WasmPtr};
use wasmer_wasi::WasiState;

use assemblylift_core::abi::RuntimeAbi;
use assemblylift_core::buffers::FunctionInputBuffer;
use assemblylift_core::threader::Threader;
use assemblylift_core::wasm::{WasmInstance, WasmMemory, WasmModule, WasmState};
use assemblylift_core_iomod::registry::RegistryTx;

mod abi;

pub type Resolver = NamedResolverChain<ImportObject, ImportObject>;

pub struct Wasmer<B, R, S>
where
    B: AsRef<[u8]>,
    R: RuntimeAbi<Vec<u8>, S> + 'static,
    S: Clone + Send + Sized + 'static,
{
    module: Module,
    store: Store,
    resolver: Option<Resolver>,
    state: Option<State<S>>,
    _phantom0: std::marker::PhantomData<B>,
    _phantom1: std::marker::PhantomData<R>,
}

impl<R, S> WasmModule<Vec<u8>, S> for Wasmer<Vec<u8>, R, S>
where
    R: RuntimeAbi<Vec<u8>, S> + 'static,
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
                _phantom0: Default::default(),
                _phantom1: Default::default(),
            }),
            Err(e) => Err(anyhow::Error::new(e)),
        }
    }

    fn deserialize_from_bytes(bytes: Vec<u8>) -> anyhow::Result<Self> {
        let compiler = Cranelift::default();
        let store = Store::new(&Universal::new(compiler).engine());
        let module = unsafe { Module::deserialize(&store, bytes.as_ref()) };
        match module {
            Ok(module) => Ok(Self {
                module,
                store,
                resolver: None,
                state: None,
                _phantom0: Default::default(),
                _phantom1: Default::default(),
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
                "__asml_abi_runtime_log" => Function::new_native_with_env(&self.store, wasm_state.clone(), log::<R, S>),
                "__asml_abi_runtime_success" => Function::new_native_with_env(&self.store, wasm_state.clone(), success::<R, S>),

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
        self.state = Some(wasm_state);
        Ok(())
    }

    fn instantiate(&self) -> anyhow::Result<Box<dyn WasmInstance>> {
        let instance = Instance::new(&self.module, &self.resolver.as_ref().unwrap()).unwrap();
        Ok(Box::new(WasmerInstance { instance }))
    }

    fn state(&self) -> &dyn WasmState<Vec<u8>, S> {
        self.state.as_ref().expect("state is None; have you called build()?")
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

impl<S> WasmState<Vec<u8>, S> for State<S>
where
    S: Clone + Send + Sized + 'static,
{
    fn threader(&self) -> MutexGuard<Threader<S>> {
        self.threader.lock().unwrap()
    }

    fn function_input_buffer(&self) -> Rc<dyn WasmMemory<Vec<u8>>> {
        Rc::new(FunctionInputMemory::new(
            self,
            Rc::new(self.memory.get_ref().cloned().unwrap()),
        ))
    }

    fn io_buffer(&self) -> Rc<dyn WasmMemory<Vec<u8>>> {
        Rc::new(IoMemory::new(
            self,
            Rc::new(self.memory.get_ref().cloned().unwrap()),
        ))
    }
}

impl<S> WasmMemory<Vec<u8>> for State<S>
where
    S: Clone + Send + Sized + 'static,
{
    fn memory_read(&self, offset: usize, length: usize) -> anyhow::Result<Vec<u8>> {
        let memory = self.memory_ref().unwrap();
        let view: MemoryView<u8> = memory.view();

        let mut bytes: Vec<u8> = Vec::new();
        for byte in view[offset as usize..(offset + length) as usize]
            .iter()
            .map(Cell::get)
        {
            bytes.push(byte);
        }

        Ok(bytes)
    }

    fn memory_write(&self, offset: usize, bytes: Vec<u8>) -> anyhow::Result<usize> {
        todo!()
    }
}

struct FunctionInputMemory<S>
where
    S: Clone + Send + Sized + 'static,
{
    ptr: WasmPtr<u8, Array>,
    mem: Rc<Memory>,
    _phantom: std::marker::PhantomData<S>,
}

impl<S> FunctionInputMemory<S>
where
    S: Clone + Send + Sized + 'static,
{
    pub fn new(state: &State<S>, mem: Rc<Memory>) -> Self {
        Self {
            ptr: state
                .get_function_input_buffer
                .get_ref()
                .unwrap()
                .call()
                .unwrap(),
            mem,
            _phantom: Default::default(),
        }
    }
}

impl<S> WasmMemory<Vec<u8>> for FunctionInputMemory<S>
where
    S: Clone + Send + Sized + 'static,
{
    fn memory_read(&self, offset: usize, length: usize) -> anyhow::Result<Vec<u8>> {
        let reader = self
            .ptr
            .deref(&*self.mem, offset as u32, length as u32)
            .unwrap();
        let bytes: Vec<u8> = reader.iter().map(|cell| cell.get()).collect_vec();
        Ok(bytes)
    }

    fn memory_write(&self, offset: usize, bytes: Vec<u8>) -> anyhow::Result<usize> {
        let writer = self
            .ptr
            .deref(&*self.mem, offset as u32, bytes.len() as u32)
            .unwrap();
        let mut bytes_out = 0usize;
        for (i, b) in bytes.iter().enumerate() {
            writer[i].set(*b);
            bytes_out += 1;
        }
        Ok(bytes_out)
    }
}

struct IoMemory<S>
where
    S: Clone + Send + Sized + 'static,
{
    ptr: WasmPtr<u8, Array>,
    mem: Rc<Memory>,
    _phantom: std::marker::PhantomData<S>,
}

impl<S> IoMemory<S>
where
    S: Clone + Send + Sized + 'static,
{
    pub fn new(state: &State<S>, mem: Rc<Memory>) -> Self {
        Self {
            ptr: state.get_io_buffer.get_ref().unwrap().call().unwrap(),
            mem,
            _phantom: Default::default(),
        }
    }
}

impl<S> WasmMemory<Vec<u8>> for IoMemory<S>
where
    S: Clone + Send + Sized + 'static,
{
    fn memory_read(&self, offset: usize, length: usize) -> anyhow::Result<Vec<u8>> {
        let reader = self
            .ptr
            .deref(&*self.mem, offset as u32, length as u32)
            .unwrap();
        let bytes: Vec<u8> = reader.iter().map(|cell| cell.get()).collect_vec();
        Ok(bytes)
    }

    fn memory_write(&self, offset: usize, bytes: Vec<u8>) -> anyhow::Result<usize> {
        let writer = self
            .ptr
            .deref(&*self.mem, offset as u32, bytes.len() as u32)
            .unwrap();
        let mut bytes_out = 0usize;
        for (i, b) in bytes.iter().enumerate() {
            writer[i].set(*b);
            bytes_out += 1;
        }
        Ok(bytes_out)
    }
}

pub struct WasmerInstance {
    instance: Instance,
}

impl WasmInstance for WasmerInstance {
    fn start(&self) -> anyhow::Result<()> {
        let start = self
            .instance
            .exports
            .get_function("_start")
            .expect("could not find WASI entrypoint in module");
        start.call(&[]).unwrap();
        Ok(()) // FIXME error handling
    }
}

pub fn compile(path: &Path) -> anyhow::Result<PathBuf> {
    // TODO compiler configuration
    let is_wasmu = path
        .extension()
        .unwrap_or("wasm".as_ref())
        .eq("wasmu");
    match is_wasmu {
        false => {
            let file_path = format!("{}u", path.display().to_string());
            println!("Precompiling WASM to {}...", file_path.clone());

            let compiler = Cranelift::default();
            let triple = Triple::from_str("x86_64-unknown-unknown").unwrap();
            let mut cpuid = CpuFeature::set();
            cpuid.insert(CpuFeature::SSE2); // required for x86
            let store = Store::new(
                &/*Native*/Universal::new(compiler)
                    .target(Target::new(triple, cpuid))
                    .engine(),
            );

            let wasm_bytes = match std::fs::read(path.clone()) {
                Ok(bytes) => bytes,
                Err(err) => panic!("{}", err.to_string()),
            };
            let module = Module::new(&store, wasm_bytes).unwrap();
            let module_bytes = module.serialize().unwrap();
            let mut module_file = match std::fs::File::create(file_path.clone()) {
                Ok(file) => file,
                Err(err) => panic!("{}", err.to_string()),
            };
            println!("📄 > Wrote {}", &file_path);
            module_file.write_all(&module_bytes).unwrap();

            Ok(PathBuf::from(file_path))
        }

        true => Ok(PathBuf::from(path)),
    }
}

fn log<R, S>(state: &State<S>, ptr: u32, len: u32)
where
    R: RuntimeAbi<Vec<u8>, S> + 'static,
    S: Clone + Send + Sized + 'static,
{
    R::log(state, ptr, len)
}

fn success<R, S>(state: &State<S>, ptr: u32, len: u32)
where
    R: RuntimeAbi<Vec<u8>, S> + 'static,
    S: Clone + Send + Sized + 'static,
{
    R::success(state, ptr, len)
}
