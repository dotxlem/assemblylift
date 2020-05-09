#[macro_use]
extern crate lazy_static;

use std::ffi::c_void;
use std::sync::Mutex;

pub mod database {
    use std::borrow::{Borrow, BorrowMut};
    use std::collections::HashMap;
    use std::ffi::c_void;
    use std::ops::DerefMut;
    use std::sync::{Arc, Mutex};

    use crossbeam_utils::atomic::AtomicCell;
    use futures::TryFutureExt;
    use rusoto_core::Region;
    use rusoto_dynamodb::{DynamoDb, DynamoDbClient, ListTablesError, ListTablesInput, ListTablesOutput};
    use wasmer_runtime::{Ctx, Func, Instance};
    use wasmer_runtime_core::vm;

    use assemblylift_core::{InstanceData, WasmBufferPtr};
    use assemblylift_core::iomod::{IoModule, ModuleRegistry};
    use assemblylift_core_event::*;
    use assemblylift_core_event::constants::EVENT_BUFFER_SIZE_BYTES;
    use assemblylift_core_event::threader::Threader;

    lazy_static! {
        static ref DYNAMODB: DynamoDbClient = DynamoDbClient::new(Region::UsEast1);
    }

    async fn __aws_dynamodb_list_tables_impl() -> Vec<u8> {
        println!("TRACE: __aws_dynamodb_list_tables_impl");

        let result = DYNAMODB.list_tables(ListTablesInput {
            exclusive_start_table_name: None,
            limit: None,
        }).await.unwrap();

        bincode::serialize(&result).unwrap()
    }

    pub fn aws_dynamodb_list_tables_impl(ctx: &mut vm::Ctx) -> i32 {
        println!("TRACE: Called aws_dynamodb_list_tables_impl");

        let mut instance_data: &mut InstanceData;
        let memory_writer: Arc<*const AtomicCell<u8>>;
        unsafe {
            instance_data = *ctx.data.cast::<&mut InstanceData>();
            // memory_writer = instance_data.memory_writer;
        }

        let mut threader = unsafe { instance_data.threader.as_mut().unwrap() };
        let event_id = threader.next_event_id().unwrap();

        // println!("TRACE: building wasm memory writer");
        // let mut __asml_get_event_buffer_pointer_func: Func<(), WasmBufferPtr> = instance.exports
        //     .get("__asml_get_event_buffer_pointer")
        //     .expect("__asml_get_event_buffer_pointer");

        // let wasm_instance_memory = ctx.memory(0);
        // let event_buffer = __asml_get_event_buffer_pointer_func.call().unwrap();
        // let memory_writer: &[AtomicCell<u8>] = event_buffer
        //     .deref(wasm_instance_memory, 0, EVENT_BUFFER_SIZE_BYTES as u32)
        //     .unwrap();

        println!("TRACE: spawning event for aws_dynamodb_list_tables_impl");
        // threader.spawn_with_event_id(Arc::from(&memory_writer[0] as *const AtomicCell<u8>), __aws_dynamodb_list_tables_impl(), event_id);
        threader.spawn_with_event_id(instance_data.memory_writer, __aws_dynamodb_list_tables_impl(), event_id);

        // (*instance_data.memory_writer.clone() as &[AtomicCell<u8>])[0].store(42u8);
        // println!("VALUE: {}", (*instance_data.memory_writer as &[AtomicCell<u8>])[0].load());

        event_id as i32
    }

    pub struct MyModule {}

    impl IoModule for MyModule {
        fn register(registry: &mut ModuleRegistry) {
            // TODO a lot of this can be hidden by a macro

            let org = "aws".to_string();
            let namespace = "dynamodb".to_string();
            let list_tables_name = "list_tables".to_string();

            let mut name_map = HashMap::new();
            name_map.entry(list_tables_name).or_insert(aws_dynamodb_list_tables_impl as fn(&mut vm::Ctx) -> i32);

            let mut namespace_map = HashMap::new();
            namespace_map.entry(namespace).or_insert(name_map);

            registry.modules.entry(org).or_insert(namespace_map);
        }
    }

}
