use std::path::Path;
use assemblylift_core::wasm::WasmModule;

pub struct Module;

impl WasmModule for Module {
    fn deserialize_from_path<P: AsRef<Path>>(&self, path: P) -> anyhow::Result<()> {
        todo!()
    }

    fn deserialize_from_bytes<B: AsRef<[u8]>>(&self, bytes: B) -> anyhow::Result<()> {
        todo!()
    }

    fn build(&self) -> anyhow::Result<()> {
        todo!()
    }

    fn instantiate(&self) -> anyhow::Result<()> {
        todo!()
    }
}
