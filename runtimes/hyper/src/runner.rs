use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::rc::Rc;

use tokio::sync::mpsc;
use tracing::{debug, info};

use assemblylift_core::wasm::{StatusTx, Wasmtime};
use assemblylift_core_iomod::registry::RegistryTx;

use crate::abi::Abi;
use crate::Status;

pub type RunnerTx<S> = mpsc::Sender<RunnerMessage<S>>;
pub type RunnerRx<S> = mpsc::Receiver<RunnerMessage<S>>;
pub type RunnerChannel<S> = (RunnerTx<S>, RunnerRx<S>);

#[derive(Clone)]
pub struct RunnerMessage<S>
where
    S: Clone + Send + Sized + 'static,
{
    pub input: Vec<u8>,
    pub status_sender: StatusTx<S>,
    pub wasm_path: PathBuf,
}

pub struct Runner<S>
where
    S: Clone + Send + Sized + 'static,
{
    channel: RunnerChannel<S>,
    registry_tx: RegistryTx,
    runtime: tokio::runtime::Runtime,
}

impl Runner<Status> {
    pub fn new(registry_tx: RegistryTx) -> Self {
        Runner {
            channel: mpsc::channel(32),
            registry_tx,
            runtime: tokio::runtime::Builder::new_current_thread()
                .enable_io()
                .build()
                .unwrap(),
        }
    }

    pub fn spawn<'a>(&mut self) {
        info!("Spawning runner");
        tokio::task::LocalSet::new().block_on(&self.runtime, async {
            let mut functions: BTreeMap<PathBuf, Rc<RefCell<Wasmtime<Abi, Status>>>> =
                BTreeMap::new();

            while let Some(msg) = self.channel.1.recv().await {
                debug!("received runner message");

                let wasm_path = match std::env::var("ASML_WASM_MODULE_NAME") {
                    Ok(module_name) => PathBuf::from(format!("/opt/assemblylift/{}", module_name)),
                    Err(_) => msg.wasm_path.clone(),
                };

                let wasmtime = match functions.contains_key(&*wasm_path) {
                    false => {
                        let wt = Rc::new(RefCell::new(
                            Wasmtime::<Abi, Status>::new_from_path(wasm_path.as_ref())
                                .expect("could not create WASM runtime from module path"),
                        ));
                        functions.insert(wasm_path, wt.clone());
                        wt
                    }
                    true => functions.get(&*wasm_path).unwrap().clone(),
                };

                let (instance, mut store) = wasmtime
                    .borrow_mut()
                    .link_wasi_component(self.registry_tx.clone(), msg.status_sender.clone(), None)
                    .await
                    .expect("could not link wasm module");

                wasmtime
                    .borrow_mut()
                    .initialize_function_input_buffer(&mut store, &msg.input)
                    .expect("could not initialize input buffer");

                let wasmtime = wasmtime.clone();
                tokio::task::spawn_local(async move {
                    match wasmtime.borrow_mut().run(instance, &mut store).await {
                        Ok(_) => msg.status_sender.send(Status::Exited(0)),
                        Err(_) => msg.status_sender.send(Status::Failure(
                            "WASM module exited in error".as_bytes().to_vec(),
                        )),
                    }
                });
            }
        });
    }

    pub fn sender(&self) -> RunnerTx<Status> {
        self.channel.0.clone()
    }
}
