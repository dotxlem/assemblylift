use std::path::Path;
use std::rc::Rc;
use std::sync::MutexGuard;

use assemblylift_core_iomod::registry::RegistryTx;

use crate::threader::Threader;

// pub type ModuleTreble<B, S> = (Module, Resolver, ThreaderEnv<B, S>);

pub trait WasmModule<S> {
    fn deserialize_from_path<P: AsRef<Path>>(path: P) -> anyhow::Result<Self>
    where
        Self: Sized;

    fn deserialize_from_bytes<B: AsRef<[u8]>>(bytes: B) -> anyhow::Result<Self>
    where
        Self: Sized;

    fn build(
        &mut self,
        registry_tx: RegistryTx,
        status_tx: crossbeam_channel::Sender<S>,
    ) -> anyhow::Result<()>;
    
    fn instantiate(&self) -> anyhow::Result<()>;
}

pub trait WasmState<B, S>
where
    B: AsRef<[u8]>,
    S: Clone + Send + Sized + 'static,
{
    fn threader(&self) -> MutexGuard<Threader<S>>;
    fn function_input_buffer(&self) -> Rc<dyn WasmMemory<B>>;
    fn io_buffer(&self) -> Rc<dyn WasmMemory<B>>;
}

pub trait WasmMemory<B>
where
    B: AsRef<[u8]>,
{
    fn memory_read(&self, offset: usize, length: usize) -> anyhow::Result<B>;
    fn memory_write(&self, offset: usize, bytes: B) -> anyhow::Result<usize>;
}

/*

pub fn new_instance(
    module: Arc<Module>,
    import_object: Resolver,
) -> Result<Instance, InstantiationError> {
    Instance::new(&module, &import_object)
}

pub fn precompile(module_path: PathBuf) -> Result<PathBuf, &'static str> {
    // TODO compiler configuration
    let is_wasmu = module_path
        .extension()
        .unwrap_or("wasm".as_ref())
        .eq("wasmu");
    match is_wasmu {
        false => {
            let file_path = format!("{}u", module_path.as_path().display().to_string());
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

            let wasm_bytes = match std::fs::read(module_path.clone()) {
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

        true => Ok(module_path),
    }
}
**/
