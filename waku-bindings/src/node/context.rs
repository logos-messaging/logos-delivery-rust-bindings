use std::ffi::c_void;
use std::ptr::null_mut;
use std::sync::{Arc, Mutex};

use crate::general::libwaku_response::LibwakuResponse;
use crate::macros::get_trampoline;

type LibwakuResponseClosure = dyn FnMut(LibwakuResponse) + Send + Sync;

pub struct WakuNodeContext {
    obj_ptr: *mut c_void,
    msg_observer: Arc<Mutex<Box<LibwakuResponseClosure>>>,
}

impl WakuNodeContext {
    pub fn new(obj_ptr: *mut c_void) -> Self {
        let me = Self {
            obj_ptr,
            msg_observer: Arc::new(Mutex::new(Box::new(|_| {}))),
        };

        // By default we set a callback that will panic if the user didn't specify a valid callback.
        // And by valid callback we mean a callback that can properly handle the waku events.
        me.waku_set_event_callback(WakuNodeContext::panic_callback)
            .expect("correctly set default callback");
        me
    }

    // default callback that does nothing. A valid callback should be set
    fn panic_callback(_response: LibwakuResponse) {
        panic!("callback not set. Please use waku_set_event_callback to set a valid callback")
    }

    pub fn get_ptr(&self) -> *mut c_void {
        self.obj_ptr
    }

    pub fn reset_ptr(mut self) {
        self.obj_ptr = null_mut();
    }

    /// Register callback to act as event handler and receive application events,
    /// which are used to react to asynchronous events in Waku
    pub fn waku_set_event_callback<F: FnMut(LibwakuResponse) + 'static + Sync + Send>(
        &self,
        closure: F,
    ) -> Result<(), String> {
        if let Ok(mut boxed_closure) = self.msg_observer.lock() {
            *boxed_closure = Box::new(closure);
            unsafe {
                let cb = get_trampoline(&(*boxed_closure));
                waku_sys::set_event_callback(
                    self.obj_ptr,
                    cb,
                    &mut (*boxed_closure) as *mut _ as *mut c_void,
                )
            };
            Ok(())
        } else {
            Err("Failed to acquire lock in waku_set_event_callback!".to_string())
        }
    }
}
