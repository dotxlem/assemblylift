#[macro_use]
extern crate lazy_static;

use std::ffi::c_void;
use std::future::Future;
use std::io::Read;
use std::ops::Deref;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::task::{Context, Poll, Waker};

use bincode::deserialize;
use embedded_executor::{AllocExecutor, SpinSleep};
use futures::FutureExt;
use futures::task::{ArcWake, waker_ref};
use lock_api::{GuardSend, Mutex, RawMutex};

use assemblylift_core_event_common::{EventHandles, EventStatus};

extern {
    fn __asml_abi_poll(id: u32) -> i32;
}

const MAX_EVENTS: usize              = 1024;
const EVENT_SIZE_BYTES: usize        = 32;
const EVENT_BUFFER_SIZE_BYTES: usize = MAX_EVENTS * EVENT_SIZE_BYTES;

// Raw buffer holding serialized Event-Future's
pub static mut EVENT_BUFFER: [u8; EVENT_BUFFER_SIZE_BYTES] = [0; EVENT_BUFFER_SIZE_BYTES];

#[no_mangle]
pub fn __asml_get_event_buffer_pointer() -> *const u8 {
    unsafe { EVENT_BUFFER.as_ptr() }
}

#[derive(Clone)]
pub struct Event {
    pub id: u32,
    waker: Option<Waker>
}

impl Event {
    pub fn new(id: u32) -> Self {
        Event { id, waker: None }
    }
}

impl Future for Event {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match unsafe { get_status(self.id) } {
            true => Poll::Ready(()),
            _ => {
                self.waker = Some(cx.waker().clone());
                Poll::Pending
            }
        }
    }
}

unsafe fn get_status(id: u32) -> bool {
    if let Ok(event_status) = deserialize::<EventHandles>(&EVENT_BUFFER[0..std::mem::size_of::<EventHandles>()]) {
        for evt in event_status.iter() {
            if evt.0 == id {
                return evt.1;
            }
        }
    }

    false
}
