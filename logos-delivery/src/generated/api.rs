use std::os::raw::{c_char, c_int, c_void};
use std::slice;
use std::time::Duration;
use serde::de::DeserializeOwned;
use serde::Serialize;
use super::ffi;
use super::types::*;

fn encode_cbor<T: Serialize>(value: &T) -> Result<Vec<u8>, String> {
    let mut buf = Vec::new();
    ciborium::ser::into_writer(value, &mut buf).map_err(|e| e.to_string())?;
    Ok(buf)
}

fn decode_cbor<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, String> {
    ciborium::de::from_reader(bytes).map_err(|e| e.to_string())
}

type FFIResult = Result<Vec<u8>, String>;
type FFISender = flume::Sender<FFIResult>;

// Reconstruct the (ret, msg, len) tuple delivered by the C callback
// into a Result<Vec<u8>, String>: payload on success, UTF-8 message on error.
// `from_utf8_lossy` accepts non-UTF-8 error bytes by inserting U+FFFD; the
// alternative would be to dispatch a separate Err for invalid UTF-8, but the
// codegen contract is that Nim handlers emit `string` error payloads, so
// invalid UTF-8 here would be a Nim-side bug.
unsafe fn ffi_payload(ret: c_int, msg: *const c_char, len: usize) -> FFIResult {
    let bytes = if msg.is_null() || len == 0 {
        Vec::new()
    } else {
        slice::from_raw_parts(msg as *const u8, len).to_vec()
    };
    if ret == NIMFFI_RET_OK { Ok(bytes) }
    else        { Err(String::from_utf8_lossy(&bytes).into_owned()) }
}

// nim-ffi result-callback status codes, emitted from ffi/ret_codes.nim.
#[allow(dead_code)]
const NIMFFI_RET_OK: c_int = 0;
#[allow(dead_code)]
const NIMFFI_RET_ERR: c_int = 1;
#[allow(dead_code)]
const NIMFFI_RET_MISSING_CALLBACK: c_int = 2;
#[allow(dead_code)]
const NIMFFI_RET_STALE_WARN: c_int = 3;

unsafe extern "C" fn on_result(
    ret: c_int,
    msg: *const c_char,
    len: usize,
    user_data: *mut c_void,
) {
    // NIMFFI_RET_STALE_WARN (3) is a non-terminal progress ping: the request
    // is still running. This wrapper only delivers the final result, so ignore
    // it WITHOUT reclaiming the box — a terminal callback still owns the Sender.
    if ret == NIMFFI_RET_STALE_WARN { return; }

    // Take ownership of the boxed Sender — dropping it at end of scope
    // releases the only outstanding handle.
    let tx = Box::from_raw(user_data as *mut FFISender);

    // `tx.send` returns Err only if the awaiting future was dropped (and with it
    // the Receiver): e.g. tokio::time::timeout elapsed, a tokio::select! branch
    // lost the race, or the future was dropped before being awaited. This cannot
    // happen with the crate's own examples but may occur in arbitrary
    // downstream consumers, so we discard the Err safely.
    // Given that this is invoked from a Nim thread, we can't propagate the error by panicking or
    // returning a Result. Furthermore, an API dev may intentionally set a timeout in the await,
    // in which case is also fine to discard the send error in this case because the API user will
    // handle the timeout expiry in their own code.
    // The important part is to ensure that the callback doesn't panic or block indefinitely if the
    // receiver is gone.
    let _ = tx.send(ffi_payload(ret, msg, len));
}

fn ffi_call_sync<F>(timeout: Duration, f: F) -> FFIResult
where
    F: FnOnce(ffi::FFICallback, *mut c_void) -> c_int,
{
    let (tx, rx) = flume::bounded::<FFIResult>(1);
    let raw = Box::into_raw(Box::new(tx)) as *mut c_void;
    let ret = f(on_result, raw);
    if ret == NIMFFI_RET_MISSING_CALLBACK {
        // Callback will never fire; reclaim the box to avoid a leak.
        drop(unsafe { Box::from_raw(raw as *mut FFISender) });
        return Err("RET_MISSING_CALLBACK (internal error)".into());
    }
    match rx.recv_timeout(timeout) {
        Ok(payload) => payload,
        Err(flume::RecvTimeoutError::Timeout) =>
            Err(format!("timed out after {:?}", timeout)),
        Err(flume::RecvTimeoutError::Disconnected) =>
            Err("callback channel disconnected before delivery".into()),
    }
}

async fn ffi_call_async<F>(timeout: Duration, f: F) -> FFIResult
where
    F: FnOnce(ffi::FFICallback, *mut c_void) -> c_int,
{
    let (tx, rx) = flume::bounded::<FFIResult>(1);
    let raw = Box::into_raw(Box::new(tx)) as *mut c_void;
    let ret = f(on_result, raw);
    if ret == NIMFFI_RET_MISSING_CALLBACK {
        drop(unsafe { Box::from_raw(raw as *mut FFISender) });
        return Err("RET_MISSING_CALLBACK (internal error)".into());
    }
    match tokio::time::timeout(timeout, rx.recv_async()).await {
        Ok(Ok(payload)) => payload,
        Ok(Err(_)) => Err("callback channel disconnected before delivery".into()),
        Err(_) => Err(format!("timed out after {:?}", timeout)),
    }
}

/// High-level context for `LogosDelivery`.
pub struct LogosDeliveryCtx {
    pub(crate) ptr: *mut c_void,
    timeout: Duration,
}

// SAFETY: The `ptr` field points to an FFIContext owned by the Nim runtime.
// Every call through the generated FFI proc goes through
// `sendRequestToFFIThread` on the Nim side, which only enqueues the request
// onto a mutex-guarded MPSC queue (sound from any number of threads) and
// wakes the single FFI thread that dispatches every handler. The context is
// thus never mutated non-atomically from the caller's thread. The Nim-side
// reentrancy guard (`onFFIThread` threadvar) prevents handlers from
// re-entering the dispatcher. These invariants make it sound to mark the
// wrapper as Send + Sync.
unsafe impl Send for LogosDeliveryCtx {}
unsafe impl Sync for LogosDeliveryCtx {}

impl Drop for LogosDeliveryCtx {
    fn drop(&mut self) {
        if !self.ptr.is_null() {
            unsafe { ffi::logosdelivery_destroy(self.ptr); }
            self.ptr = std::ptr::null_mut();
        }
    }
}

impl LogosDeliveryCtx {
    pub fn create(config_json: String, timeout: Duration) -> Result<Self, String> {
        let req = LogosdeliveryCreateNodeCtorReq { config_json };
        let req_bytes = encode_cbor(&req)?;
        let raw_bytes = ffi_call_sync(timeout, |cb, ud| unsafe {
            let _ = ffi::logosdelivery_create_node(req_bytes.as_ptr(), req_bytes.len(), cb, ud);
            0
        })?;
        let addr_str: String = decode_cbor(&raw_bytes)?;
        let addr: usize = addr_str.parse().map_err(|e: std::num::ParseIntError| e.to_string())?;
        Ok(Self { ptr: addr as *mut c_void, timeout })
    }

    pub async fn new_async(config_json: String, timeout: Duration) -> Result<Self, String> {
        let req = LogosdeliveryCreateNodeCtorReq { config_json };
        let req_bytes = encode_cbor(&req)?;
        let raw_bytes = ffi_call_async(timeout, move |cb, ud| unsafe {
            let _ = ffi::logosdelivery_create_node(req_bytes.as_ptr(), req_bytes.len(), cb, ud);
            0
        }).await?;
        let addr_str: String = decode_cbor(&raw_bytes)?;
        let addr: usize = addr_str.parse().map_err(|e: std::num::ParseIntError| e.to_string())?;
        Ok(Self { ptr: addr as *mut c_void, timeout })
    }

    pub fn start_node(&self) -> Result<String, String> {
        let req = LogosdeliveryStartNodeReq {};
        let req_bytes = encode_cbor(&req)?;
        let raw_bytes = ffi_call_sync(self.timeout, |cb, ud| unsafe {
            ffi::logosdelivery_start_node(self.ptr, cb, ud, req_bytes.as_ptr(), req_bytes.len())
        })?;
        decode_cbor::<String>(&raw_bytes)
    }

    pub async fn start_node_async(&self) -> Result<String, String> {
        let req = LogosdeliveryStartNodeReq {};
        let req_bytes = encode_cbor(&req)?;
        let ptr = self.ptr as usize;
        let raw_bytes = ffi_call_async(self.timeout, move |cb, ud| unsafe {
            ffi::logosdelivery_start_node(ptr as *mut c_void, cb, ud, req_bytes.as_ptr(), req_bytes.len())
        }).await?;
        decode_cbor::<String>(&raw_bytes)
    }

    pub fn stop_node(&self) -> Result<String, String> {
        let req = LogosdeliveryStopNodeReq {};
        let req_bytes = encode_cbor(&req)?;
        let raw_bytes = ffi_call_sync(self.timeout, |cb, ud| unsafe {
            ffi::logosdelivery_stop_node(self.ptr, cb, ud, req_bytes.as_ptr(), req_bytes.len())
        })?;
        decode_cbor::<String>(&raw_bytes)
    }

    pub async fn stop_node_async(&self) -> Result<String, String> {
        let req = LogosdeliveryStopNodeReq {};
        let req_bytes = encode_cbor(&req)?;
        let ptr = self.ptr as usize;
        let raw_bytes = ffi_call_async(self.timeout, move |cb, ud| unsafe {
            ffi::logosdelivery_stop_node(ptr as *mut c_void, cb, ud, req_bytes.as_ptr(), req_bytes.len())
        }).await?;
        decode_cbor::<String>(&raw_bytes)
    }

    /// Returns the node's current connection status: `Disconnected`,
    /// `PartiallyConnected` or `Connected`. `onConnectionStatusChange` reports
    /// only transitions, so this is how a late listener reads the current one.
    pub fn get_connection_status(&self) -> Result<String, String> {
        let req = LogosdeliveryGetConnectionStatusReq {};
        let req_bytes = encode_cbor(&req)?;
        let raw_bytes = ffi_call_sync(self.timeout, |cb, ud| unsafe {
            ffi::logosdelivery_get_connection_status(self.ptr, cb, ud, req_bytes.as_ptr(), req_bytes.len())
        })?;
        decode_cbor::<String>(&raw_bytes)
    }

    /// Returns the node's current connection status: `Disconnected`,
    /// `PartiallyConnected` or `Connected`. `onConnectionStatusChange` reports
    /// only transitions, so this is how a late listener reads the current one.
    pub async fn get_connection_status_async(&self) -> Result<String, String> {
        let req = LogosdeliveryGetConnectionStatusReq {};
        let req_bytes = encode_cbor(&req)?;
        let ptr = self.ptr as usize;
        let raw_bytes = ffi_call_async(self.timeout, move |cb, ud| unsafe {
            ffi::logosdelivery_get_connection_status(ptr as *mut c_void, cb, ud, req_bytes.as_ptr(), req_bytes.len())
        }).await?;
        decode_cbor::<String>(&raw_bytes)
    }

    pub fn subscribe(&self, content_topic_str: String) -> Result<String, String> {
        let req = LogosdeliverySubscribeReq { content_topic_str };
        let req_bytes = encode_cbor(&req)?;
        let raw_bytes = ffi_call_sync(self.timeout, |cb, ud| unsafe {
            ffi::logosdelivery_subscribe(self.ptr, cb, ud, req_bytes.as_ptr(), req_bytes.len())
        })?;
        decode_cbor::<String>(&raw_bytes)
    }

    pub async fn subscribe_async(&self, content_topic_str: String) -> Result<String, String> {
        let req = LogosdeliverySubscribeReq { content_topic_str };
        let req_bytes = encode_cbor(&req)?;
        let ptr = self.ptr as usize;
        let raw_bytes = ffi_call_async(self.timeout, move |cb, ud| unsafe {
            ffi::logosdelivery_subscribe(ptr as *mut c_void, cb, ud, req_bytes.as_ptr(), req_bytes.len())
        }).await?;
        decode_cbor::<String>(&raw_bytes)
    }

    pub fn unsubscribe(&self, content_topic_str: String) -> Result<String, String> {
        let req = LogosdeliveryUnsubscribeReq { content_topic_str };
        let req_bytes = encode_cbor(&req)?;
        let raw_bytes = ffi_call_sync(self.timeout, |cb, ud| unsafe {
            ffi::logosdelivery_unsubscribe(self.ptr, cb, ud, req_bytes.as_ptr(), req_bytes.len())
        })?;
        decode_cbor::<String>(&raw_bytes)
    }

    pub async fn unsubscribe_async(&self, content_topic_str: String) -> Result<String, String> {
        let req = LogosdeliveryUnsubscribeReq { content_topic_str };
        let req_bytes = encode_cbor(&req)?;
        let ptr = self.ptr as usize;
        let raw_bytes = ffi_call_async(self.timeout, move |cb, ud| unsafe {
            ffi::logosdelivery_unsubscribe(ptr as *mut c_void, cb, ud, req_bytes.as_ptr(), req_bytes.len())
        }).await?;
        decode_cbor::<String>(&raw_bytes)
    }

    pub fn send(&self, message_json: String) -> Result<String, String> {
        let req = LogosdeliverySendReq { message_json };
        let req_bytes = encode_cbor(&req)?;
        let raw_bytes = ffi_call_sync(self.timeout, |cb, ud| unsafe {
            ffi::logosdelivery_send(self.ptr, cb, ud, req_bytes.as_ptr(), req_bytes.len())
        })?;
        decode_cbor::<String>(&raw_bytes)
    }

    pub async fn send_async(&self, message_json: String) -> Result<String, String> {
        let req = LogosdeliverySendReq { message_json };
        let req_bytes = encode_cbor(&req)?;
        let ptr = self.ptr as usize;
        let raw_bytes = ffi_call_async(self.timeout, move |cb, ud| unsafe {
            ffi::logosdelivery_send(ptr as *mut c_void, cb, ud, req_bytes.as_ptr(), req_bytes.len())
        }).await?;
        decode_cbor::<String>(&raw_bytes)
    }

    /// Returns, as a JSON array of strings, all available node info item ids that
    /// can be queried with `get_node_info`.
    pub fn get_available_node_info_ids(&self) -> Result<String, String> {
        let req = LogosdeliveryGetAvailableNodeInfoIdsReq {};
        let req_bytes = encode_cbor(&req)?;
        let raw_bytes = ffi_call_sync(self.timeout, |cb, ud| unsafe {
            ffi::logosdelivery_get_available_node_info_ids(self.ptr, cb, ud, req_bytes.as_ptr(), req_bytes.len())
        })?;
        decode_cbor::<String>(&raw_bytes)
    }

    /// Returns, as a JSON array of strings, all available node info item ids that
    /// can be queried with `get_node_info`.
    pub async fn get_available_node_info_ids_async(&self) -> Result<String, String> {
        let req = LogosdeliveryGetAvailableNodeInfoIdsReq {};
        let req_bytes = encode_cbor(&req)?;
        let ptr = self.ptr as usize;
        let raw_bytes = ffi_call_async(self.timeout, move |cb, ud| unsafe {
            ffi::logosdelivery_get_available_node_info_ids(ptr as *mut c_void, cb, ud, req_bytes.as_ptr(), req_bytes.len())
        }).await?;
        decode_cbor::<String>(&raw_bytes)
    }

    /// Returns the content of the node info item with the given id if it exists.
    /// The content is a plain string, not JSON: a peer id, an ENR URI, a
    /// comma-separated multiaddress list or the Prometheus metrics text.
    pub fn get_node_info(&self, node_info_id: String) -> Result<String, String> {
        let req = LogosdeliveryGetNodeInfoReq { node_info_id };
        let req_bytes = encode_cbor(&req)?;
        let raw_bytes = ffi_call_sync(self.timeout, |cb, ud| unsafe {
            ffi::logosdelivery_get_node_info(self.ptr, cb, ud, req_bytes.as_ptr(), req_bytes.len())
        })?;
        decode_cbor::<String>(&raw_bytes)
    }

    /// Returns the content of the node info item with the given id if it exists.
    /// The content is a plain string, not JSON: a peer id, an ENR URI, a
    /// comma-separated multiaddress list or the Prometheus metrics text.
    pub async fn get_node_info_async(&self, node_info_id: String) -> Result<String, String> {
        let req = LogosdeliveryGetNodeInfoReq { node_info_id };
        let req_bytes = encode_cbor(&req)?;
        let ptr = self.ptr as usize;
        let raw_bytes = ffi_call_async(self.timeout, move |cb, ud| unsafe {
            ffi::logosdelivery_get_node_info(ptr as *mut c_void, cb, ud, req_bytes.as_ptr(), req_bytes.len())
        }).await?;
        decode_cbor::<String>(&raw_bytes)
    }

    /// Returns information about the accepted config items.
    pub fn get_available_configs(&self) -> Result<String, String> {
        let req = LogosdeliveryGetAvailableConfigsReq {};
        let req_bytes = encode_cbor(&req)?;
        let raw_bytes = ffi_call_sync(self.timeout, |cb, ud| unsafe {
            ffi::logosdelivery_get_available_configs(self.ptr, cb, ud, req_bytes.as_ptr(), req_bytes.len())
        })?;
        decode_cbor::<String>(&raw_bytes)
    }

    /// Returns information about the accepted config items.
    pub async fn get_available_configs_async(&self) -> Result<String, String> {
        let req = LogosdeliveryGetAvailableConfigsReq {};
        let req_bytes = encode_cbor(&req)?;
        let ptr = self.ptr as usize;
        let raw_bytes = ffi_call_async(self.timeout, move |cb, ud| unsafe {
            ffi::logosdelivery_get_available_configs(ptr as *mut c_void, cb, ud, req_bytes.as_ptr(), req_bytes.len())
        }).await?;
        decode_cbor::<String>(&raw_bytes)
    }

    /// Installs (or replaces) the service-discovery plugin.
    /// pluginPtr - address of an `LdServiceDiscoveryPlugin`, borrowed for the call
    pub fn set_service_discovery_plugin(&self, plugin_ptr: u64) -> Result<String, String> {
        let req = LogosdeliverySetServiceDiscoveryPluginReq { plugin_ptr };
        let req_bytes = encode_cbor(&req)?;
        let raw_bytes = ffi_call_sync(self.timeout, |cb, ud| unsafe {
            ffi::logosdelivery_set_service_discovery_plugin(self.ptr, cb, ud, req_bytes.as_ptr(), req_bytes.len())
        })?;
        decode_cbor::<String>(&raw_bytes)
    }

    /// Installs (or replaces) the service-discovery plugin.
    /// pluginPtr - address of an `LdServiceDiscoveryPlugin`, borrowed for the call
    pub async fn set_service_discovery_plugin_async(&self, plugin_ptr: u64) -> Result<String, String> {
        let req = LogosdeliverySetServiceDiscoveryPluginReq { plugin_ptr };
        let req_bytes = encode_cbor(&req)?;
        let ptr = self.ptr as usize;
        let raw_bytes = ffi_call_async(self.timeout, move |cb, ud| unsafe {
            ffi::logosdelivery_set_service_discovery_plugin(ptr as *mut c_void, cb, ud, req_bytes.as_ptr(), req_bytes.len())
        }).await?;
        decode_cbor::<String>(&raw_bytes)
    }

    /// What the host must set up before `start`: whether service discovery is
    /// expected from a plugin, and the DHT bootstrap peers the node's
    /// configuration resolves to, presets included. Reply JSON:
    /// {"externalServiceDiscovery": bool, "bootstrapNodes": ["/dns4/.../p2p/16Uiu..."]}
    pub fn get_discovery_requirements(&self) -> Result<String, String> {
        let req = LogosdeliveryGetDiscoveryRequirementsReq {};
        let req_bytes = encode_cbor(&req)?;
        let raw_bytes = ffi_call_sync(self.timeout, |cb, ud| unsafe {
            ffi::logosdelivery_get_discovery_requirements(self.ptr, cb, ud, req_bytes.as_ptr(), req_bytes.len())
        })?;
        decode_cbor::<String>(&raw_bytes)
    }

    /// What the host must set up before `start`: whether service discovery is
    /// expected from a plugin, and the DHT bootstrap peers the node's
    /// configuration resolves to, presets included. Reply JSON:
    /// {"externalServiceDiscovery": bool, "bootstrapNodes": ["/dns4/.../p2p/16Uiu..."]}
    pub async fn get_discovery_requirements_async(&self) -> Result<String, String> {
        let req = LogosdeliveryGetDiscoveryRequirementsReq {};
        let req_bytes = encode_cbor(&req)?;
        let ptr = self.ptr as usize;
        let raw_bytes = ffi_call_async(self.timeout, move |cb, ud| unsafe {
            ffi::logosdelivery_get_discovery_requirements(ptr as *mut c_void, cb, ud, req_bytes.as_ptr(), req_bytes.len())
        }).await?;
        decode_cbor::<String>(&raw_bytes)
    }

    /// Removes the installed plugin; discovery verbs fail until a new one arrives.
    pub fn clear_service_discovery_plugin(&self) -> Result<String, String> {
        let req = LogosdeliveryClearServiceDiscoveryPluginReq {};
        let req_bytes = encode_cbor(&req)?;
        let raw_bytes = ffi_call_sync(self.timeout, |cb, ud| unsafe {
            ffi::logosdelivery_clear_service_discovery_plugin(self.ptr, cb, ud, req_bytes.as_ptr(), req_bytes.len())
        })?;
        decode_cbor::<String>(&raw_bytes)
    }

    /// Removes the installed plugin; discovery verbs fail until a new one arrives.
    pub async fn clear_service_discovery_plugin_async(&self) -> Result<String, String> {
        let req = LogosdeliveryClearServiceDiscoveryPluginReq {};
        let req_bytes = encode_cbor(&req)?;
        let ptr = self.ptr as usize;
        let raw_bytes = ffi_call_async(self.timeout, move |cb, ud| unsafe {
            ffi::logosdelivery_clear_service_discovery_plugin(ptr as *mut c_void, cb, ud, req_bytes.as_ptr(), req_bytes.len())
        }).await?;
        decode_cbor::<String>(&raw_bytes)
    }

    /// returns a comma-separated string of peerIDs
    pub fn waku_get_peerids_from_peerstore(&self) -> Result<String, String> {
        let req = WakuGetPeeridsFromPeerstoreReq {};
        let req_bytes = encode_cbor(&req)?;
        let raw_bytes = ffi_call_sync(self.timeout, |cb, ud| unsafe {
            ffi::waku_get_peerids_from_peerstore(self.ptr, cb, ud, req_bytes.as_ptr(), req_bytes.len())
        })?;
        decode_cbor::<String>(&raw_bytes)
    }

    /// returns a comma-separated string of peerIDs
    pub async fn waku_get_peerids_from_peerstore_async(&self) -> Result<String, String> {
        let req = WakuGetPeeridsFromPeerstoreReq {};
        let req_bytes = encode_cbor(&req)?;
        let ptr = self.ptr as usize;
        let raw_bytes = ffi_call_async(self.timeout, move |cb, ud| unsafe {
            ffi::waku_get_peerids_from_peerstore(ptr as *mut c_void, cb, ud, req_bytes.as_ptr(), req_bytes.len())
        }).await?;
        decode_cbor::<String>(&raw_bytes)
    }

    pub fn waku_connect(&self, peer_multi_addr: String, timeout_ms: u32) -> Result<String, String> {
        let req = WakuConnectReq { peer_multi_addr, timeout_ms };
        let req_bytes = encode_cbor(&req)?;
        let raw_bytes = ffi_call_sync(self.timeout, |cb, ud| unsafe {
            ffi::waku_connect(self.ptr, cb, ud, req_bytes.as_ptr(), req_bytes.len())
        })?;
        decode_cbor::<String>(&raw_bytes)
    }

    pub async fn waku_connect_async(&self, peer_multi_addr: String, timeout_ms: u32) -> Result<String, String> {
        let req = WakuConnectReq { peer_multi_addr, timeout_ms };
        let req_bytes = encode_cbor(&req)?;
        let ptr = self.ptr as usize;
        let raw_bytes = ffi_call_async(self.timeout, move |cb, ud| unsafe {
            ffi::waku_connect(ptr as *mut c_void, cb, ud, req_bytes.as_ptr(), req_bytes.len())
        }).await?;
        decode_cbor::<String>(&raw_bytes)
    }

    pub fn waku_disconnect_peer_by_id(&self, peer_id: String) -> Result<String, String> {
        let req = WakuDisconnectPeerByIdReq { peer_id };
        let req_bytes = encode_cbor(&req)?;
        let raw_bytes = ffi_call_sync(self.timeout, |cb, ud| unsafe {
            ffi::waku_disconnect_peer_by_id(self.ptr, cb, ud, req_bytes.as_ptr(), req_bytes.len())
        })?;
        decode_cbor::<String>(&raw_bytes)
    }

    pub async fn waku_disconnect_peer_by_id_async(&self, peer_id: String) -> Result<String, String> {
        let req = WakuDisconnectPeerByIdReq { peer_id };
        let req_bytes = encode_cbor(&req)?;
        let ptr = self.ptr as usize;
        let raw_bytes = ffi_call_async(self.timeout, move |cb, ud| unsafe {
            ffi::waku_disconnect_peer_by_id(ptr as *mut c_void, cb, ud, req_bytes.as_ptr(), req_bytes.len())
        }).await?;
        decode_cbor::<String>(&raw_bytes)
    }

    pub fn waku_disconnect_all_peers(&self) -> Result<String, String> {
        let req = WakuDisconnectAllPeersReq {};
        let req_bytes = encode_cbor(&req)?;
        let raw_bytes = ffi_call_sync(self.timeout, |cb, ud| unsafe {
            ffi::waku_disconnect_all_peers(self.ptr, cb, ud, req_bytes.as_ptr(), req_bytes.len())
        })?;
        decode_cbor::<String>(&raw_bytes)
    }

    pub async fn waku_disconnect_all_peers_async(&self) -> Result<String, String> {
        let req = WakuDisconnectAllPeersReq {};
        let req_bytes = encode_cbor(&req)?;
        let ptr = self.ptr as usize;
        let raw_bytes = ffi_call_async(self.timeout, move |cb, ud| unsafe {
            ffi::waku_disconnect_all_peers(ptr as *mut c_void, cb, ud, req_bytes.as_ptr(), req_bytes.len())
        }).await?;
        decode_cbor::<String>(&raw_bytes)
    }

    pub fn waku_dial_peer(&self, peer_multi_addr: String, protocol: String, timeout_ms: u32) -> Result<String, String> {
        let req = WakuDialPeerReq { peer_multi_addr, protocol, timeout_ms };
        let req_bytes = encode_cbor(&req)?;
        let raw_bytes = ffi_call_sync(self.timeout, |cb, ud| unsafe {
            ffi::waku_dial_peer(self.ptr, cb, ud, req_bytes.as_ptr(), req_bytes.len())
        })?;
        decode_cbor::<String>(&raw_bytes)
    }

    pub async fn waku_dial_peer_async(&self, peer_multi_addr: String, protocol: String, timeout_ms: u32) -> Result<String, String> {
        let req = WakuDialPeerReq { peer_multi_addr, protocol, timeout_ms };
        let req_bytes = encode_cbor(&req)?;
        let ptr = self.ptr as usize;
        let raw_bytes = ffi_call_async(self.timeout, move |cb, ud| unsafe {
            ffi::waku_dial_peer(ptr as *mut c_void, cb, ud, req_bytes.as_ptr(), req_bytes.len())
        }).await?;
        decode_cbor::<String>(&raw_bytes)
    }

    pub fn waku_dial_peer_by_id(&self, peer_id: String, protocol: String, timeout_ms: u32) -> Result<String, String> {
        let req = WakuDialPeerByIdReq { peer_id, protocol, timeout_ms };
        let req_bytes = encode_cbor(&req)?;
        let raw_bytes = ffi_call_sync(self.timeout, |cb, ud| unsafe {
            ffi::waku_dial_peer_by_id(self.ptr, cb, ud, req_bytes.as_ptr(), req_bytes.len())
        })?;
        decode_cbor::<String>(&raw_bytes)
    }

    pub async fn waku_dial_peer_by_id_async(&self, peer_id: String, protocol: String, timeout_ms: u32) -> Result<String, String> {
        let req = WakuDialPeerByIdReq { peer_id, protocol, timeout_ms };
        let req_bytes = encode_cbor(&req)?;
        let ptr = self.ptr as usize;
        let raw_bytes = ffi_call_async(self.timeout, move |cb, ud| unsafe {
            ffi::waku_dial_peer_by_id(ptr as *mut c_void, cb, ud, req_bytes.as_ptr(), req_bytes.len())
        }).await?;
        decode_cbor::<String>(&raw_bytes)
    }

    /// returns a JSON string mapping peerIDs to objects with protocols and addresses
    pub fn waku_get_connected_peers_info(&self) -> Result<String, String> {
        let req = WakuGetConnectedPeersInfoReq {};
        let req_bytes = encode_cbor(&req)?;
        let raw_bytes = ffi_call_sync(self.timeout, |cb, ud| unsafe {
            ffi::waku_get_connected_peers_info(self.ptr, cb, ud, req_bytes.as_ptr(), req_bytes.len())
        })?;
        decode_cbor::<String>(&raw_bytes)
    }

    /// returns a JSON string mapping peerIDs to objects with protocols and addresses
    pub async fn waku_get_connected_peers_info_async(&self) -> Result<String, String> {
        let req = WakuGetConnectedPeersInfoReq {};
        let req_bytes = encode_cbor(&req)?;
        let ptr = self.ptr as usize;
        let raw_bytes = ffi_call_async(self.timeout, move |cb, ud| unsafe {
            ffi::waku_get_connected_peers_info(ptr as *mut c_void, cb, ud, req_bytes.as_ptr(), req_bytes.len())
        }).await?;
        decode_cbor::<String>(&raw_bytes)
    }

    /// returns a comma-separated string of peerIDs
    pub fn waku_get_connected_peers(&self) -> Result<String, String> {
        let req = WakuGetConnectedPeersReq {};
        let req_bytes = encode_cbor(&req)?;
        let raw_bytes = ffi_call_sync(self.timeout, |cb, ud| unsafe {
            ffi::waku_get_connected_peers(self.ptr, cb, ud, req_bytes.as_ptr(), req_bytes.len())
        })?;
        decode_cbor::<String>(&raw_bytes)
    }

    /// returns a comma-separated string of peerIDs
    pub async fn waku_get_connected_peers_async(&self) -> Result<String, String> {
        let req = WakuGetConnectedPeersReq {};
        let req_bytes = encode_cbor(&req)?;
        let ptr = self.ptr as usize;
        let raw_bytes = ffi_call_async(self.timeout, move |cb, ud| unsafe {
            ffi::waku_get_connected_peers(ptr as *mut c_void, cb, ud, req_bytes.as_ptr(), req_bytes.len())
        }).await?;
        decode_cbor::<String>(&raw_bytes)
    }

    /// returns a comma-separated string of peerIDs that mount the given protocol
    pub fn waku_get_peerids_by_protocol(&self, protocol: String) -> Result<String, String> {
        let req = WakuGetPeeridsByProtocolReq { protocol };
        let req_bytes = encode_cbor(&req)?;
        let raw_bytes = ffi_call_sync(self.timeout, |cb, ud| unsafe {
            ffi::waku_get_peerids_by_protocol(self.ptr, cb, ud, req_bytes.as_ptr(), req_bytes.len())
        })?;
        decode_cbor::<String>(&raw_bytes)
    }

    /// returns a comma-separated string of peerIDs that mount the given protocol
    pub async fn waku_get_peerids_by_protocol_async(&self, protocol: String) -> Result<String, String> {
        let req = WakuGetPeeridsByProtocolReq { protocol };
        let req_bytes = encode_cbor(&req)?;
        let ptr = self.ptr as usize;
        let raw_bytes = ffi_call_async(self.timeout, move |cb, ud| unsafe {
            ffi::waku_get_peerids_by_protocol(ptr as *mut c_void, cb, ud, req_bytes.as_ptr(), req_bytes.len())
        }).await?;
        decode_cbor::<String>(&raw_bytes)
    }

    /// Updates the bootnode list used for discovering new peers via DiscoveryV5
    /// bootnodes - JSON array containing the bootnode ENRs i.e. `["enr:...", "enr:..."]`
    pub fn waku_discv5_update_bootnodes(&self, bootnodes: String) -> Result<String, String> {
        let req = WakuDiscv5UpdateBootnodesReq { bootnodes };
        let req_bytes = encode_cbor(&req)?;
        let raw_bytes = ffi_call_sync(self.timeout, |cb, ud| unsafe {
            ffi::waku_discv5_update_bootnodes(self.ptr, cb, ud, req_bytes.as_ptr(), req_bytes.len())
        })?;
        decode_cbor::<String>(&raw_bytes)
    }

    /// Updates the bootnode list used for discovering new peers via DiscoveryV5
    /// bootnodes - JSON array containing the bootnode ENRs i.e. `["enr:...", "enr:..."]`
    pub async fn waku_discv5_update_bootnodes_async(&self, bootnodes: String) -> Result<String, String> {
        let req = WakuDiscv5UpdateBootnodesReq { bootnodes };
        let req_bytes = encode_cbor(&req)?;
        let ptr = self.ptr as usize;
        let raw_bytes = ffi_call_async(self.timeout, move |cb, ud| unsafe {
            ffi::waku_discv5_update_bootnodes(ptr as *mut c_void, cb, ud, req_bytes.as_ptr(), req_bytes.len())
        }).await?;
        decode_cbor::<String>(&raw_bytes)
    }

    /// returns a comma-separated string of bootstrap nodes' multiaddresses
    pub fn waku_dns_discovery(&self, enr_tree_url: String, name_dns_server: String, timeout_ms: i32) -> Result<String, String> {
        let req = WakuDnsDiscoveryReq { enr_tree_url, name_dns_server, timeout_ms };
        let req_bytes = encode_cbor(&req)?;
        let raw_bytes = ffi_call_sync(self.timeout, |cb, ud| unsafe {
            ffi::waku_dns_discovery(self.ptr, cb, ud, req_bytes.as_ptr(), req_bytes.len())
        })?;
        decode_cbor::<String>(&raw_bytes)
    }

    /// returns a comma-separated string of bootstrap nodes' multiaddresses
    pub async fn waku_dns_discovery_async(&self, enr_tree_url: String, name_dns_server: String, timeout_ms: i32) -> Result<String, String> {
        let req = WakuDnsDiscoveryReq { enr_tree_url, name_dns_server, timeout_ms };
        let req_bytes = encode_cbor(&req)?;
        let ptr = self.ptr as usize;
        let raw_bytes = ffi_call_async(self.timeout, move |cb, ud| unsafe {
            ffi::waku_dns_discovery(ptr as *mut c_void, cb, ud, req_bytes.as_ptr(), req_bytes.len())
        }).await?;
        decode_cbor::<String>(&raw_bytes)
    }

    pub fn waku_start_discv5(&self) -> Result<String, String> {
        let req = WakuStartDiscv5Req {};
        let req_bytes = encode_cbor(&req)?;
        let raw_bytes = ffi_call_sync(self.timeout, |cb, ud| unsafe {
            ffi::waku_start_discv5(self.ptr, cb, ud, req_bytes.as_ptr(), req_bytes.len())
        })?;
        decode_cbor::<String>(&raw_bytes)
    }

    pub async fn waku_start_discv5_async(&self) -> Result<String, String> {
        let req = WakuStartDiscv5Req {};
        let req_bytes = encode_cbor(&req)?;
        let ptr = self.ptr as usize;
        let raw_bytes = ffi_call_async(self.timeout, move |cb, ud| unsafe {
            ffi::waku_start_discv5(ptr as *mut c_void, cb, ud, req_bytes.as_ptr(), req_bytes.len())
        }).await?;
        decode_cbor::<String>(&raw_bytes)
    }

    pub fn waku_stop_discv5(&self) -> Result<String, String> {
        let req = WakuStopDiscv5Req {};
        let req_bytes = encode_cbor(&req)?;
        let raw_bytes = ffi_call_sync(self.timeout, |cb, ud| unsafe {
            ffi::waku_stop_discv5(self.ptr, cb, ud, req_bytes.as_ptr(), req_bytes.len())
        })?;
        decode_cbor::<String>(&raw_bytes)
    }

    pub async fn waku_stop_discv5_async(&self) -> Result<String, String> {
        let req = WakuStopDiscv5Req {};
        let req_bytes = encode_cbor(&req)?;
        let ptr = self.ptr as usize;
        let raw_bytes = ffi_call_async(self.timeout, move |cb, ud| unsafe {
            ffi::waku_stop_discv5(ptr as *mut c_void, cb, ud, req_bytes.as_ptr(), req_bytes.len())
        }).await?;
        decode_cbor::<String>(&raw_bytes)
    }

    pub fn waku_peer_exchange_request(&self, num_peers: u64) -> Result<String, String> {
        let req = WakuPeerExchangeRequestReq { num_peers };
        let req_bytes = encode_cbor(&req)?;
        let raw_bytes = ffi_call_sync(self.timeout, |cb, ud| unsafe {
            ffi::waku_peer_exchange_request(self.ptr, cb, ud, req_bytes.as_ptr(), req_bytes.len())
        })?;
        decode_cbor::<String>(&raw_bytes)
    }

    pub async fn waku_peer_exchange_request_async(&self, num_peers: u64) -> Result<String, String> {
        let req = WakuPeerExchangeRequestReq { num_peers };
        let req_bytes = encode_cbor(&req)?;
        let ptr = self.ptr as usize;
        let raw_bytes = ffi_call_async(self.timeout, move |cb, ud| unsafe {
            ffi::waku_peer_exchange_request(ptr as *mut c_void, cb, ud, req_bytes.as_ptr(), req_bytes.len())
        }).await?;
        decode_cbor::<String>(&raw_bytes)
    }

    pub fn waku_version(&self) -> Result<String, String> {
        let req = WakuVersionReq {};
        let req_bytes = encode_cbor(&req)?;
        let raw_bytes = ffi_call_sync(self.timeout, |cb, ud| unsafe {
            ffi::waku_version(self.ptr, cb, ud, req_bytes.as_ptr(), req_bytes.len())
        })?;
        decode_cbor::<String>(&raw_bytes)
    }

    pub async fn waku_version_async(&self) -> Result<String, String> {
        let req = WakuVersionReq {};
        let req_bytes = encode_cbor(&req)?;
        let ptr = self.ptr as usize;
        let raw_bytes = ffi_call_async(self.timeout, move |cb, ud| unsafe {
            ffi::waku_version(ptr as *mut c_void, cb, ud, req_bytes.as_ptr(), req_bytes.len())
        }).await?;
        decode_cbor::<String>(&raw_bytes)
    }

    /// returns a comma-separated string of the listen addresses
    pub fn waku_listen_addresses(&self) -> Result<String, String> {
        let req = WakuListenAddressesReq {};
        let req_bytes = encode_cbor(&req)?;
        let raw_bytes = ffi_call_sync(self.timeout, |cb, ud| unsafe {
            ffi::waku_listen_addresses(self.ptr, cb, ud, req_bytes.as_ptr(), req_bytes.len())
        })?;
        decode_cbor::<String>(&raw_bytes)
    }

    /// returns a comma-separated string of the listen addresses
    pub async fn waku_listen_addresses_async(&self) -> Result<String, String> {
        let req = WakuListenAddressesReq {};
        let req_bytes = encode_cbor(&req)?;
        let ptr = self.ptr as usize;
        let raw_bytes = ffi_call_async(self.timeout, move |cb, ud| unsafe {
            ffi::waku_listen_addresses(ptr as *mut c_void, cb, ud, req_bytes.as_ptr(), req_bytes.len())
        }).await?;
        decode_cbor::<String>(&raw_bytes)
    }

    pub fn waku_get_my_enr(&self) -> Result<String, String> {
        let req = WakuGetMyEnrReq {};
        let req_bytes = encode_cbor(&req)?;
        let raw_bytes = ffi_call_sync(self.timeout, |cb, ud| unsafe {
            ffi::waku_get_my_enr(self.ptr, cb, ud, req_bytes.as_ptr(), req_bytes.len())
        })?;
        decode_cbor::<String>(&raw_bytes)
    }

    pub async fn waku_get_my_enr_async(&self) -> Result<String, String> {
        let req = WakuGetMyEnrReq {};
        let req_bytes = encode_cbor(&req)?;
        let ptr = self.ptr as usize;
        let raw_bytes = ffi_call_async(self.timeout, move |cb, ud| unsafe {
            ffi::waku_get_my_enr(ptr as *mut c_void, cb, ud, req_bytes.as_ptr(), req_bytes.len())
        }).await?;
        decode_cbor::<String>(&raw_bytes)
    }

    pub fn waku_get_my_peerid(&self) -> Result<String, String> {
        let req = WakuGetMyPeeridReq {};
        let req_bytes = encode_cbor(&req)?;
        let raw_bytes = ffi_call_sync(self.timeout, |cb, ud| unsafe {
            ffi::waku_get_my_peerid(self.ptr, cb, ud, req_bytes.as_ptr(), req_bytes.len())
        })?;
        decode_cbor::<String>(&raw_bytes)
    }

    pub async fn waku_get_my_peerid_async(&self) -> Result<String, String> {
        let req = WakuGetMyPeeridReq {};
        let req_bytes = encode_cbor(&req)?;
        let ptr = self.ptr as usize;
        let raw_bytes = ffi_call_async(self.timeout, move |cb, ud| unsafe {
            ffi::waku_get_my_peerid(ptr as *mut c_void, cb, ud, req_bytes.as_ptr(), req_bytes.len())
        }).await?;
        decode_cbor::<String>(&raw_bytes)
    }

    pub fn waku_get_metrics(&self) -> Result<String, String> {
        let req = WakuGetMetricsReq {};
        let req_bytes = encode_cbor(&req)?;
        let raw_bytes = ffi_call_sync(self.timeout, |cb, ud| unsafe {
            ffi::waku_get_metrics(self.ptr, cb, ud, req_bytes.as_ptr(), req_bytes.len())
        })?;
        decode_cbor::<String>(&raw_bytes)
    }

    pub async fn waku_get_metrics_async(&self) -> Result<String, String> {
        let req = WakuGetMetricsReq {};
        let req_bytes = encode_cbor(&req)?;
        let ptr = self.ptr as usize;
        let raw_bytes = ffi_call_async(self.timeout, move |cb, ud| unsafe {
            ffi::waku_get_metrics(ptr as *mut c_void, cb, ud, req_bytes.as_ptr(), req_bytes.len())
        }).await?;
        decode_cbor::<String>(&raw_bytes)
    }

    pub fn waku_is_online(&self) -> Result<String, String> {
        let req = WakuIsOnlineReq {};
        let req_bytes = encode_cbor(&req)?;
        let raw_bytes = ffi_call_sync(self.timeout, |cb, ud| unsafe {
            ffi::waku_is_online(self.ptr, cb, ud, req_bytes.as_ptr(), req_bytes.len())
        })?;
        decode_cbor::<String>(&raw_bytes)
    }

    pub async fn waku_is_online_async(&self) -> Result<String, String> {
        let req = WakuIsOnlineReq {};
        let req_bytes = encode_cbor(&req)?;
        let ptr = self.ptr as usize;
        let raw_bytes = ffi_call_async(self.timeout, move |cb, ud| unsafe {
            ffi::waku_is_online(ptr as *mut c_void, cb, ud, req_bytes.as_ptr(), req_bytes.len())
        }).await?;
        decode_cbor::<String>(&raw_bytes)
    }

    pub fn waku_ping_peer(&self, peer_addr: String, timeout_ms: u32) -> Result<String, String> {
        let req = WakuPingPeerReq { peer_addr, timeout_ms };
        let req_bytes = encode_cbor(&req)?;
        let raw_bytes = ffi_call_sync(self.timeout, |cb, ud| unsafe {
            ffi::waku_ping_peer(self.ptr, cb, ud, req_bytes.as_ptr(), req_bytes.len())
        })?;
        decode_cbor::<String>(&raw_bytes)
    }

    pub async fn waku_ping_peer_async(&self, peer_addr: String, timeout_ms: u32) -> Result<String, String> {
        let req = WakuPingPeerReq { peer_addr, timeout_ms };
        let req_bytes = encode_cbor(&req)?;
        let ptr = self.ptr as usize;
        let raw_bytes = ffi_call_async(self.timeout, move |cb, ud| unsafe {
            ffi::waku_ping_peer(ptr as *mut c_void, cb, ud, req_bytes.as_ptr(), req_bytes.len())
        }).await?;
        decode_cbor::<String>(&raw_bytes)
    }

    /// returns a comma-separated string of peerIDs
    pub fn waku_relay_get_peers_in_mesh(&self, pub_sub_topic: String) -> Result<String, String> {
        let req = WakuRelayGetPeersInMeshReq { pub_sub_topic };
        let req_bytes = encode_cbor(&req)?;
        let raw_bytes = ffi_call_sync(self.timeout, |cb, ud| unsafe {
            ffi::waku_relay_get_peers_in_mesh(self.ptr, cb, ud, req_bytes.as_ptr(), req_bytes.len())
        })?;
        decode_cbor::<String>(&raw_bytes)
    }

    /// returns a comma-separated string of peerIDs
    pub async fn waku_relay_get_peers_in_mesh_async(&self, pub_sub_topic: String) -> Result<String, String> {
        let req = WakuRelayGetPeersInMeshReq { pub_sub_topic };
        let req_bytes = encode_cbor(&req)?;
        let ptr = self.ptr as usize;
        let raw_bytes = ffi_call_async(self.timeout, move |cb, ud| unsafe {
            ffi::waku_relay_get_peers_in_mesh(ptr as *mut c_void, cb, ud, req_bytes.as_ptr(), req_bytes.len())
        }).await?;
        decode_cbor::<String>(&raw_bytes)
    }

    pub fn waku_relay_get_num_peers_in_mesh(&self, pub_sub_topic: String) -> Result<String, String> {
        let req = WakuRelayGetNumPeersInMeshReq { pub_sub_topic };
        let req_bytes = encode_cbor(&req)?;
        let raw_bytes = ffi_call_sync(self.timeout, |cb, ud| unsafe {
            ffi::waku_relay_get_num_peers_in_mesh(self.ptr, cb, ud, req_bytes.as_ptr(), req_bytes.len())
        })?;
        decode_cbor::<String>(&raw_bytes)
    }

    pub async fn waku_relay_get_num_peers_in_mesh_async(&self, pub_sub_topic: String) -> Result<String, String> {
        let req = WakuRelayGetNumPeersInMeshReq { pub_sub_topic };
        let req_bytes = encode_cbor(&req)?;
        let ptr = self.ptr as usize;
        let raw_bytes = ffi_call_async(self.timeout, move |cb, ud| unsafe {
            ffi::waku_relay_get_num_peers_in_mesh(ptr as *mut c_void, cb, ud, req_bytes.as_ptr(), req_bytes.len())
        }).await?;
        decode_cbor::<String>(&raw_bytes)
    }

    /// Returns the list of all connected peers to an specific pubsub topic
    pub fn waku_relay_get_connected_peers(&self, pub_sub_topic: String) -> Result<String, String> {
        let req = WakuRelayGetConnectedPeersReq { pub_sub_topic };
        let req_bytes = encode_cbor(&req)?;
        let raw_bytes = ffi_call_sync(self.timeout, |cb, ud| unsafe {
            ffi::waku_relay_get_connected_peers(self.ptr, cb, ud, req_bytes.as_ptr(), req_bytes.len())
        })?;
        decode_cbor::<String>(&raw_bytes)
    }

    /// Returns the list of all connected peers to an specific pubsub topic
    pub async fn waku_relay_get_connected_peers_async(&self, pub_sub_topic: String) -> Result<String, String> {
        let req = WakuRelayGetConnectedPeersReq { pub_sub_topic };
        let req_bytes = encode_cbor(&req)?;
        let ptr = self.ptr as usize;
        let raw_bytes = ffi_call_async(self.timeout, move |cb, ud| unsafe {
            ffi::waku_relay_get_connected_peers(ptr as *mut c_void, cb, ud, req_bytes.as_ptr(), req_bytes.len())
        }).await?;
        decode_cbor::<String>(&raw_bytes)
    }

    pub fn waku_relay_get_num_connected_peers(&self, pub_sub_topic: String) -> Result<String, String> {
        let req = WakuRelayGetNumConnectedPeersReq { pub_sub_topic };
        let req_bytes = encode_cbor(&req)?;
        let raw_bytes = ffi_call_sync(self.timeout, |cb, ud| unsafe {
            ffi::waku_relay_get_num_connected_peers(self.ptr, cb, ud, req_bytes.as_ptr(), req_bytes.len())
        })?;
        decode_cbor::<String>(&raw_bytes)
    }

    pub async fn waku_relay_get_num_connected_peers_async(&self, pub_sub_topic: String) -> Result<String, String> {
        let req = WakuRelayGetNumConnectedPeersReq { pub_sub_topic };
        let req_bytes = encode_cbor(&req)?;
        let ptr = self.ptr as usize;
        let raw_bytes = ffi_call_async(self.timeout, move |cb, ud| unsafe {
            ffi::waku_relay_get_num_connected_peers(ptr as *mut c_void, cb, ud, req_bytes.as_ptr(), req_bytes.len())
        }).await?;
        decode_cbor::<String>(&raw_bytes)
    }

    /// Protects a shard with a public key
    pub fn waku_relay_add_protected_shard(&self, cluster_id: u16, shard_id: u16, public_key: String) -> Result<String, String> {
        let req = WakuRelayAddProtectedShardReq { cluster_id, shard_id, public_key };
        let req_bytes = encode_cbor(&req)?;
        let raw_bytes = ffi_call_sync(self.timeout, |cb, ud| unsafe {
            ffi::waku_relay_add_protected_shard(self.ptr, cb, ud, req_bytes.as_ptr(), req_bytes.len())
        })?;
        decode_cbor::<String>(&raw_bytes)
    }

    /// Protects a shard with a public key
    pub async fn waku_relay_add_protected_shard_async(&self, cluster_id: u16, shard_id: u16, public_key: String) -> Result<String, String> {
        let req = WakuRelayAddProtectedShardReq { cluster_id, shard_id, public_key };
        let req_bytes = encode_cbor(&req)?;
        let ptr = self.ptr as usize;
        let raw_bytes = ffi_call_async(self.timeout, move |cb, ud| unsafe {
            ffi::waku_relay_add_protected_shard(ptr as *mut c_void, cb, ud, req_bytes.as_ptr(), req_bytes.len())
        }).await?;
        decode_cbor::<String>(&raw_bytes)
    }

    pub fn waku_relay_subscribe(&self, pub_sub_topic: String) -> Result<String, String> {
        let req = WakuRelaySubscribeReq { pub_sub_topic };
        let req_bytes = encode_cbor(&req)?;
        let raw_bytes = ffi_call_sync(self.timeout, |cb, ud| unsafe {
            ffi::waku_relay_subscribe(self.ptr, cb, ud, req_bytes.as_ptr(), req_bytes.len())
        })?;
        decode_cbor::<String>(&raw_bytes)
    }

    pub async fn waku_relay_subscribe_async(&self, pub_sub_topic: String) -> Result<String, String> {
        let req = WakuRelaySubscribeReq { pub_sub_topic };
        let req_bytes = encode_cbor(&req)?;
        let ptr = self.ptr as usize;
        let raw_bytes = ffi_call_async(self.timeout, move |cb, ud| unsafe {
            ffi::waku_relay_subscribe(ptr as *mut c_void, cb, ud, req_bytes.as_ptr(), req_bytes.len())
        }).await?;
        decode_cbor::<String>(&raw_bytes)
    }

    pub fn waku_relay_unsubscribe(&self, pub_sub_topic: String) -> Result<String, String> {
        let req = WakuRelayUnsubscribeReq { pub_sub_topic };
        let req_bytes = encode_cbor(&req)?;
        let raw_bytes = ffi_call_sync(self.timeout, |cb, ud| unsafe {
            ffi::waku_relay_unsubscribe(self.ptr, cb, ud, req_bytes.as_ptr(), req_bytes.len())
        })?;
        decode_cbor::<String>(&raw_bytes)
    }

    pub async fn waku_relay_unsubscribe_async(&self, pub_sub_topic: String) -> Result<String, String> {
        let req = WakuRelayUnsubscribeReq { pub_sub_topic };
        let req_bytes = encode_cbor(&req)?;
        let ptr = self.ptr as usize;
        let raw_bytes = ffi_call_async(self.timeout, move |cb, ud| unsafe {
            ffi::waku_relay_unsubscribe(ptr as *mut c_void, cb, ud, req_bytes.as_ptr(), req_bytes.len())
        }).await?;
        decode_cbor::<String>(&raw_bytes)
    }

    pub fn waku_relay_publish(&self, pub_sub_topic: String, json_waku_message: String, timeout_ms: u32) -> Result<String, String> {
        let req = WakuRelayPublishReq { pub_sub_topic, json_waku_message, timeout_ms };
        let req_bytes = encode_cbor(&req)?;
        let raw_bytes = ffi_call_sync(self.timeout, |cb, ud| unsafe {
            ffi::waku_relay_publish(self.ptr, cb, ud, req_bytes.as_ptr(), req_bytes.len())
        })?;
        decode_cbor::<String>(&raw_bytes)
    }

    pub async fn waku_relay_publish_async(&self, pub_sub_topic: String, json_waku_message: String, timeout_ms: u32) -> Result<String, String> {
        let req = WakuRelayPublishReq { pub_sub_topic, json_waku_message, timeout_ms };
        let req_bytes = encode_cbor(&req)?;
        let ptr = self.ptr as usize;
        let raw_bytes = ffi_call_async(self.timeout, move |cb, ud| unsafe {
            ffi::waku_relay_publish(ptr as *mut c_void, cb, ud, req_bytes.as_ptr(), req_bytes.len())
        }).await?;
        decode_cbor::<String>(&raw_bytes)
    }

    pub fn waku_default_pubsub_topic(&self) -> Result<String, String> {
        let req = WakuDefaultPubsubTopicReq {};
        let req_bytes = encode_cbor(&req)?;
        let raw_bytes = ffi_call_sync(self.timeout, |cb, ud| unsafe {
            ffi::waku_default_pubsub_topic(self.ptr, cb, ud, req_bytes.as_ptr(), req_bytes.len())
        })?;
        decode_cbor::<String>(&raw_bytes)
    }

    pub async fn waku_default_pubsub_topic_async(&self) -> Result<String, String> {
        let req = WakuDefaultPubsubTopicReq {};
        let req_bytes = encode_cbor(&req)?;
        let ptr = self.ptr as usize;
        let raw_bytes = ffi_call_async(self.timeout, move |cb, ud| unsafe {
            ffi::waku_default_pubsub_topic(ptr as *mut c_void, cb, ud, req_bytes.as_ptr(), req_bytes.len())
        }).await?;
        decode_cbor::<String>(&raw_bytes)
    }

    pub fn waku_content_topic(&self, app_name: String, app_version: u32, content_topic_name: String, encoding: String) -> Result<String, String> {
        let req = WakuContentTopicReq { app_name, app_version, content_topic_name, encoding };
        let req_bytes = encode_cbor(&req)?;
        let raw_bytes = ffi_call_sync(self.timeout, |cb, ud| unsafe {
            ffi::waku_content_topic(self.ptr, cb, ud, req_bytes.as_ptr(), req_bytes.len())
        })?;
        decode_cbor::<String>(&raw_bytes)
    }

    pub async fn waku_content_topic_async(&self, app_name: String, app_version: u32, content_topic_name: String, encoding: String) -> Result<String, String> {
        let req = WakuContentTopicReq { app_name, app_version, content_topic_name, encoding };
        let req_bytes = encode_cbor(&req)?;
        let ptr = self.ptr as usize;
        let raw_bytes = ffi_call_async(self.timeout, move |cb, ud| unsafe {
            ffi::waku_content_topic(ptr as *mut c_void, cb, ud, req_bytes.as_ptr(), req_bytes.len())
        }).await?;
        decode_cbor::<String>(&raw_bytes)
    }

    pub fn waku_pubsub_topic(&self, topic_name: String) -> Result<String, String> {
        let req = WakuPubsubTopicReq { topic_name };
        let req_bytes = encode_cbor(&req)?;
        let raw_bytes = ffi_call_sync(self.timeout, |cb, ud| unsafe {
            ffi::waku_pubsub_topic(self.ptr, cb, ud, req_bytes.as_ptr(), req_bytes.len())
        })?;
        decode_cbor::<String>(&raw_bytes)
    }

    pub async fn waku_pubsub_topic_async(&self, topic_name: String) -> Result<String, String> {
        let req = WakuPubsubTopicReq { topic_name };
        let req_bytes = encode_cbor(&req)?;
        let ptr = self.ptr as usize;
        let raw_bytes = ffi_call_async(self.timeout, move |cb, ud| unsafe {
            ffi::waku_pubsub_topic(ptr as *mut c_void, cb, ud, req_bytes.as_ptr(), req_bytes.len())
        }).await?;
        decode_cbor::<String>(&raw_bytes)
    }

    pub fn waku_store_query(&self, json_query: String, peer_addr: String, timeout_ms: i32) -> Result<String, String> {
        let req = WakuStoreQueryReq { json_query, peer_addr, timeout_ms };
        let req_bytes = encode_cbor(&req)?;
        let raw_bytes = ffi_call_sync(self.timeout, |cb, ud| unsafe {
            ffi::waku_store_query(self.ptr, cb, ud, req_bytes.as_ptr(), req_bytes.len())
        })?;
        decode_cbor::<String>(&raw_bytes)
    }

    pub async fn waku_store_query_async(&self, json_query: String, peer_addr: String, timeout_ms: i32) -> Result<String, String> {
        let req = WakuStoreQueryReq { json_query, peer_addr, timeout_ms };
        let req_bytes = encode_cbor(&req)?;
        let ptr = self.ptr as usize;
        let raw_bytes = ffi_call_async(self.timeout, move |cb, ud| unsafe {
            ffi::waku_store_query(ptr as *mut c_void, cb, ud, req_bytes.as_ptr(), req_bytes.len())
        }).await?;
        decode_cbor::<String>(&raw_bytes)
    }

    pub fn waku_lightpush_publish(&self, pub_sub_topic: String, json_waku_message: String) -> Result<String, String> {
        let req = WakuLightpushPublishReq { pub_sub_topic, json_waku_message };
        let req_bytes = encode_cbor(&req)?;
        let raw_bytes = ffi_call_sync(self.timeout, |cb, ud| unsafe {
            ffi::waku_lightpush_publish(self.ptr, cb, ud, req_bytes.as_ptr(), req_bytes.len())
        })?;
        decode_cbor::<String>(&raw_bytes)
    }

    pub async fn waku_lightpush_publish_async(&self, pub_sub_topic: String, json_waku_message: String) -> Result<String, String> {
        let req = WakuLightpushPublishReq { pub_sub_topic, json_waku_message };
        let req_bytes = encode_cbor(&req)?;
        let ptr = self.ptr as usize;
        let raw_bytes = ffi_call_async(self.timeout, move |cb, ud| unsafe {
            ffi::waku_lightpush_publish(ptr as *mut c_void, cb, ud, req_bytes.as_ptr(), req_bytes.len())
        }).await?;
        decode_cbor::<String>(&raw_bytes)
    }

    pub fn waku_filter_subscribe(&self, pub_sub_topic: String, content_topics: String) -> Result<String, String> {
        let req = WakuFilterSubscribeReq { pub_sub_topic, content_topics };
        let req_bytes = encode_cbor(&req)?;
        let raw_bytes = ffi_call_sync(self.timeout, |cb, ud| unsafe {
            ffi::waku_filter_subscribe(self.ptr, cb, ud, req_bytes.as_ptr(), req_bytes.len())
        })?;
        decode_cbor::<String>(&raw_bytes)
    }

    pub async fn waku_filter_subscribe_async(&self, pub_sub_topic: String, content_topics: String) -> Result<String, String> {
        let req = WakuFilterSubscribeReq { pub_sub_topic, content_topics };
        let req_bytes = encode_cbor(&req)?;
        let ptr = self.ptr as usize;
        let raw_bytes = ffi_call_async(self.timeout, move |cb, ud| unsafe {
            ffi::waku_filter_subscribe(ptr as *mut c_void, cb, ud, req_bytes.as_ptr(), req_bytes.len())
        }).await?;
        decode_cbor::<String>(&raw_bytes)
    }

    pub fn waku_filter_unsubscribe(&self, pub_sub_topic: String, content_topics: String) -> Result<String, String> {
        let req = WakuFilterUnsubscribeReq { pub_sub_topic, content_topics };
        let req_bytes = encode_cbor(&req)?;
        let raw_bytes = ffi_call_sync(self.timeout, |cb, ud| unsafe {
            ffi::waku_filter_unsubscribe(self.ptr, cb, ud, req_bytes.as_ptr(), req_bytes.len())
        })?;
        decode_cbor::<String>(&raw_bytes)
    }

    pub async fn waku_filter_unsubscribe_async(&self, pub_sub_topic: String, content_topics: String) -> Result<String, String> {
        let req = WakuFilterUnsubscribeReq { pub_sub_topic, content_topics };
        let req_bytes = encode_cbor(&req)?;
        let ptr = self.ptr as usize;
        let raw_bytes = ffi_call_async(self.timeout, move |cb, ud| unsafe {
            ffi::waku_filter_unsubscribe(ptr as *mut c_void, cb, ud, req_bytes.as_ptr(), req_bytes.len())
        }).await?;
        decode_cbor::<String>(&raw_bytes)
    }

    pub fn waku_filter_unsubscribe_all(&self) -> Result<String, String> {
        let req = WakuFilterUnsubscribeAllReq {};
        let req_bytes = encode_cbor(&req)?;
        let raw_bytes = ffi_call_sync(self.timeout, |cb, ud| unsafe {
            ffi::waku_filter_unsubscribe_all(self.ptr, cb, ud, req_bytes.as_ptr(), req_bytes.len())
        })?;
        decode_cbor::<String>(&raw_bytes)
    }

    pub async fn waku_filter_unsubscribe_all_async(&self) -> Result<String, String> {
        let req = WakuFilterUnsubscribeAllReq {};
        let req_bytes = encode_cbor(&req)?;
        let ptr = self.ptr as usize;
        let raw_bytes = ffi_call_async(self.timeout, move |cb, ud| unsafe {
            ffi::waku_filter_unsubscribe_all(ptr as *mut c_void, cb, ud, req_bytes.as_ptr(), req_bytes.len())
        }).await?;
        decode_cbor::<String>(&raw_bytes)
    }

    /// `encryptFn`/`decryptFn` are `LogosDeliveryCryptoFn` pointers cast to
    /// `uint64`, and all three zero means an unencrypted channel. The cipher
    /// is fixed for the channel's life.
    ///
    /// `userData` is what lets one C function serve several channels: a
    /// function pointer carries no state, so the same `my_encrypt` used on two
    /// channels is the same address both times and cannot tell them apart.
    /// Whatever is passed here comes back as the callback's first argument on
    /// every call, so it can point at this channel's key.
    pub fn channel_create(&self, channel_id_str: String, content_topic_str: String, sender_id_str: String, encrypt_fn: u64, decrypt_fn: u64, user_data: u64) -> Result<String, String> {
        let req = LogosdeliveryChannelCreateReq { channel_id_str, content_topic_str, sender_id_str, encrypt_fn, decrypt_fn, user_data };
        let req_bytes = encode_cbor(&req)?;
        let raw_bytes = ffi_call_sync(self.timeout, |cb, ud| unsafe {
            ffi::logosdelivery_channel_create(self.ptr, cb, ud, req_bytes.as_ptr(), req_bytes.len())
        })?;
        decode_cbor::<String>(&raw_bytes)
    }

    /// `encryptFn`/`decryptFn` are `LogosDeliveryCryptoFn` pointers cast to
    /// `uint64`, and all three zero means an unencrypted channel. The cipher
    /// is fixed for the channel's life.
    ///
    /// `userData` is what lets one C function serve several channels: a
    /// function pointer carries no state, so the same `my_encrypt` used on two
    /// channels is the same address both times and cannot tell them apart.
    /// Whatever is passed here comes back as the callback's first argument on
    /// every call, so it can point at this channel's key.
    pub async fn channel_create_async(&self, channel_id_str: String, content_topic_str: String, sender_id_str: String, encrypt_fn: u64, decrypt_fn: u64, user_data: u64) -> Result<String, String> {
        let req = LogosdeliveryChannelCreateReq { channel_id_str, content_topic_str, sender_id_str, encrypt_fn, decrypt_fn, user_data };
        let req_bytes = encode_cbor(&req)?;
        let ptr = self.ptr as usize;
        let raw_bytes = ffi_call_async(self.timeout, move |cb, ud| unsafe {
            ffi::logosdelivery_channel_create(ptr as *mut c_void, cb, ud, req_bytes.as_ptr(), req_bytes.len())
        }).await?;
        decode_cbor::<String>(&raw_bytes)
    }

    /// Returns `"true"` or `"false"`; a missing channel is not an error.
    pub fn channel_exists(&self, channel_id_str: String) -> Result<String, String> {
        let req = LogosdeliveryChannelExistsReq { channel_id_str };
        let req_bytes = encode_cbor(&req)?;
        let raw_bytes = ffi_call_sync(self.timeout, |cb, ud| unsafe {
            ffi::logosdelivery_channel_exists(self.ptr, cb, ud, req_bytes.as_ptr(), req_bytes.len())
        })?;
        decode_cbor::<String>(&raw_bytes)
    }

    /// Returns `"true"` or `"false"`; a missing channel is not an error.
    pub async fn channel_exists_async(&self, channel_id_str: String) -> Result<String, String> {
        let req = LogosdeliveryChannelExistsReq { channel_id_str };
        let req_bytes = encode_cbor(&req)?;
        let ptr = self.ptr as usize;
        let raw_bytes = ffi_call_async(self.timeout, move |cb, ud| unsafe {
            ffi::logosdelivery_channel_exists(ptr as *mut c_void, cb, ud, req_bytes.as_ptr(), req_bytes.len())
        }).await?;
        decode_cbor::<String>(&raw_bytes)
    }

    /// `messageJson` carries `{ "payload": <base64>, "ephemeral": <bool> }`.
    pub fn channel_send(&self, channel_id_str: String, message_json: String) -> Result<String, String> {
        let req = LogosdeliveryChannelSendReq { channel_id_str, message_json };
        let req_bytes = encode_cbor(&req)?;
        let raw_bytes = ffi_call_sync(self.timeout, |cb, ud| unsafe {
            ffi::logosdelivery_channel_send(self.ptr, cb, ud, req_bytes.as_ptr(), req_bytes.len())
        })?;
        decode_cbor::<String>(&raw_bytes)
    }

    /// `messageJson` carries `{ "payload": <base64>, "ephemeral": <bool> }`.
    pub async fn channel_send_async(&self, channel_id_str: String, message_json: String) -> Result<String, String> {
        let req = LogosdeliveryChannelSendReq { channel_id_str, message_json };
        let req_bytes = encode_cbor(&req)?;
        let ptr = self.ptr as usize;
        let raw_bytes = ffi_call_async(self.timeout, move |cb, ud| unsafe {
            ffi::logosdelivery_channel_send(ptr as *mut c_void, cb, ud, req_bytes.as_ptr(), req_bytes.len())
        }).await?;
        decode_cbor::<String>(&raw_bytes)
    }

    pub fn channel_close(&self, channel_id_str: String) -> Result<String, String> {
        let req = LogosdeliveryChannelCloseReq { channel_id_str };
        let req_bytes = encode_cbor(&req)?;
        let raw_bytes = ffi_call_sync(self.timeout, |cb, ud| unsafe {
            ffi::logosdelivery_channel_close(self.ptr, cb, ud, req_bytes.as_ptr(), req_bytes.len())
        })?;
        decode_cbor::<String>(&raw_bytes)
    }

    pub async fn channel_close_async(&self, channel_id_str: String) -> Result<String, String> {
        let req = LogosdeliveryChannelCloseReq { channel_id_str };
        let req_bytes = encode_cbor(&req)?;
        let ptr = self.ptr as usize;
        let raw_bytes = ffi_call_async(self.timeout, move |cb, ud| unsafe {
            ffi::logosdelivery_channel_close(ptr as *mut c_void, cb, ud, req_bytes.as_ptr(), req_bytes.len())
        }).await?;
        decode_cbor::<String>(&raw_bytes)
    }

    /// Stop every context the library still holds and join their threads.
    /// Call it before the process exits when a context is still alive, or when a
    /// static proc built the shared context.
    /// Returns 0 when every context stopped, 1 when one was left running.
    /// This wrapper reports that as true.
    pub fn shutdown() -> bool {
        unsafe { ffi::logosdelivery_shutdown() == 0 }
    }

}
