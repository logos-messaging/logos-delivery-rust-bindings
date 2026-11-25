use crate::general::libwaku_response::LibwakuResponse;

use std::{slice, str};
use waku_sys::FFICallBack;

unsafe extern "C" fn trampoline<F>(
    ret_code: ::std::os::raw::c_int,
    data: *const ::std::os::raw::c_char,
    data_len: usize,
    user_data: *mut ::std::os::raw::c_void,
) where
    F: FnMut(LibwakuResponse),
{
    let closure = &mut *(user_data as *mut F);

    let response = if data.is_null() {
        ""
    } else {
        str::from_utf8(slice::from_raw_parts(data as *mut u8, data_len))
            .expect("could not retrieve response")
    };

    let result = LibwakuResponse::try_from((ret_code as u32, response))
        .expect("invalid response obtained from libwaku");

    closure(result);
}

pub fn get_trampoline<F>(_closure: &F) -> FFICallBack
where
    F: FnMut(LibwakuResponse),
{
    Some(trampoline::<F>)
}

#[macro_export]
macro_rules! handle_ffi_call {
    // Case: With or without additional arguments
    ($waku_fn:expr, $resp_hndlr:expr, $ctx:expr $(, $($arg:expr),*)?) => {{
        use $crate::macros::get_trampoline;
        use std::sync::Arc;
        use tokio::sync::Notify;
        use libc::*;

        let mut result = LibwakuResponse::default();
        let notify = Arc::new(Notify::new());
        let notify_clone = notify.clone();

        // Callback to update the result and notify the waiter
        let result_cb = |r: LibwakuResponse| {
            result = r;
            notify_clone.notify_one();
        };

        let mut closure = result_cb;
        // Create trampoline and invoke the `waku_sys` function
        let code = unsafe {
            let cb = get_trampoline(&closure);
            $waku_fn(
                $ctx,                                   // Pass the context
                cb,                                     // Pass the callback trampoline
                &mut closure as *mut _ as *mut c_void,  // Pass the user data
                $($($arg),*)?                           // Expand the variadic arguments if provided
            )
        };

        // Wait for the callback to notify us
        notify.notified().await;

        // Handle the response
        $resp_hndlr(code, result)
    }};
}
