use std::ffi::CString;
use std::os::raw::{c_char, c_int, c_void};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use base64::Engine;
use futures_core::Stream;
use serde_json::json;
use tokio::sync::broadcast;
use tokio_stream::{wrappers::BroadcastStream, StreamExt};

use crate::channel::{Channel, ChannelConfig};
use crate::config::DeliveryConfig;
use crate::error::{DeliveryError, Result};
use crate::events::{ConnectionStatus, DeliveryEvent, ReceivedMessage, EVENT_NAMES};
use crate::generated::api::LogosDeliveryCtx;

/// Capacity of the event fan-out; a receiver that falls further behind skips events.
const EVENT_BUFFER: usize = 1024;

type Callback = unsafe extern "C" fn(c_int, *const c_char, usize, *mut c_void);

extern "C" {
    fn logosdelivery_add_event_listener(
        ctx: *mut c_void,
        event_name: *const c_char,
        cb: Callback,
        user_data: *mut c_void,
    ) -> u64;
    fn logosdelivery_remove_event_listener(ctx: *mut c_void, id: u64) -> c_int;
}

/// Identifier the node assigns to a send, echoed by the delivery events.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RequestId(pub String);

impl std::fmt::Display for RequestId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Runs on the node's event thread: must not block.
unsafe extern "C" fn on_event(ret: c_int, msg: *const c_char, len: usize, user_data: *mut c_void) {
    if ret != 0 || msg.is_null() || user_data.is_null() {
        return;
    }
    let bytes = std::slice::from_raw_parts(msg as *const u8, len);
    let tx = &*(user_data as *const broadcast::Sender<DeliveryEvent>);
    match serde_json::from_slice::<DeliveryEvent>(bytes) {
        Ok(event) => {
            // No receivers is fine: nobody asked for events yet.
            let _ = tx.send(event);
        }
        Err(e) => tracing::debug!("ignoring unparsable delivery event: {e}"),
    }
}

struct Inner {
    ctx: Option<LogosDeliveryCtx>,
    events: Arc<broadcast::Sender<DeliveryEvent>>,
    listeners: Mutex<Vec<u64>>,
    /// The `Arc` handed to the listeners as `user_data`, released once they are gone.
    listener_data: *const broadcast::Sender<DeliveryEvent>,
}

// SAFETY: the raw pointer is an `Arc` strong count owned by `Inner`; the context is
// documented thread-safe in the generated bindings.
unsafe impl Send for Inner {}
unsafe impl Sync for Inner {}

impl Inner {
    fn ctx(&self) -> &LogosDeliveryCtx {
        self.ctx.as_ref().expect("ctx is only taken on drop")
    }
}

impl Drop for Inner {
    fn drop(&mut self) {
        let ctx = self.ctx.take();
        let listeners = std::mem::take(&mut *self.listeners.lock().unwrap());
        let data = self.listener_data as usize;
        let cleanup = move || {
            if let Some(ctx) = ctx {
                for id in listeners {
                    // Returns after the last delivery to this listener, so
                    // `user_data` is safe to free below.
                    unsafe { logosdelivery_remove_event_listener(ctx.ptr, id) };
                }
                // Stops the node if still running, then frees the context.
                drop(ctx);
            }
            drop(unsafe { Arc::from_raw(data as *const broadcast::Sender<DeliveryEvent>) });
        };
        // Destroying a context blocks for as long as the node takes to stop.
        match tokio::runtime::Handle::try_current() {
            Ok(handle) => {
                handle.spawn_blocking(cleanup);
            }
            Err(_) => cleanup(),
        }
    }
}

/// A running Logos Delivery node. Cheap to clone; the node stops once the last
/// clone is dropped, or earlier through [`DeliveryNode::shutdown`].
#[derive(Clone)]
pub struct DeliveryNode {
    inner: Arc<Inner>,
}

impl DeliveryNode {
    /// Creates and starts a node. Returns once it is started, not once it is
    /// connected: see [`DeliveryNode::wait_connected`].
    pub async fn start(config: DeliveryConfig) -> Result<Self> {
        config.check_version()?;
        let ctx = LogosDeliveryCtx::new_async(config.to_json(), config.timeout())
            .await
            .map_err(DeliveryError::Startup)?;

        let (tx, _) = broadcast::channel(EVENT_BUFFER);
        let events = Arc::new(tx);
        let listener_data = Arc::into_raw(events.clone());

        // Listeners go in before start so no event is missed.
        let mut ids = Vec::with_capacity(EVENT_NAMES.len());
        for name in EVENT_NAMES {
            let cname = CString::new(*name).expect("event names have no NUL");
            let id = unsafe {
                logosdelivery_add_event_listener(
                    ctx.ptr,
                    cname.as_ptr(),
                    on_event,
                    listener_data as *mut c_void,
                )
            };
            if id != 0 {
                ids.push(id);
            }
        }

        // From here `Inner::drop` owns the cleanup, including on the error path.
        let node = Self {
            inner: Arc::new(Inner {
                ctx: Some(ctx),
                events,
                listeners: Mutex::new(ids),
                listener_data,
            }),
        };
        node.inner
            .ctx()
            .start_node_async()
            .await
            .map_err(DeliveryError::Startup)?;
        Ok(node)
    }

    /// Subscribes to events. Receivers only see events emitted after this call.
    pub fn events(&self) -> broadcast::Receiver<DeliveryEvent> {
        self.inner.events.subscribe()
    }

    /// Messages received by the node, as a stream. Skips events it lagged past.
    pub fn messages(&self) -> impl Stream<Item = ReceivedMessage> {
        BroadcastStream::new(self.events()).filter_map(|event| match event {
            Ok(DeliveryEvent::MessageReceived { message, .. }) => Some(message),
            Ok(_) => None,
            Err(e) => {
                tracing::warn!("message stream lagged: {e}");
                None
            }
        })
    }

    pub async fn subscribe(&self, content_topic: &str) -> Result<()> {
        self.inner
            .ctx()
            .subscribe_async(content_topic.to_string())
            .await
            .map(|_| ())
            .map_err(DeliveryError::Subscribe)
    }

    pub async fn unsubscribe(&self, content_topic: &str) -> Result<()> {
        self.inner
            .ctx()
            .unsubscribe_async(content_topic.to_string())
            .await
            .map(|_| ())
            .map_err(DeliveryError::Unsubscribe)
    }

    /// Publishes `payload` on `content_topic`. Follow its fate through the
    /// `Message*` events carrying the returned id.
    pub async fn publish(&self, content_topic: &str, payload: &[u8]) -> Result<RequestId> {
        let message = json!({
            "contentTopic": content_topic,
            "payload": base64::engine::general_purpose::STANDARD.encode(payload),
            "ephemeral": false,
        });
        self.inner
            .ctx()
            .send_async(message.to_string())
            .await
            .map(RequestId)
            .map_err(DeliveryError::Publish)
    }

    pub async fn connection_status(&self) -> Result<ConnectionStatus> {
        let status = self
            .inner
            .ctx()
            .get_connection_status_async()
            .await
            .map_err(DeliveryError::Startup)?;
        serde_json::from_value(serde_json::Value::String(
            status.trim_matches('"').to_string(),
        ))
        .map_err(|e| DeliveryError::Startup(e.to_string()))
    }

    /// Waits until the node reports at least one connection.
    pub async fn wait_connected(&self, timeout: Duration) -> Result<ConnectionStatus> {
        // Subscribe first, so a transition between the two reads is not lost.
        let mut events = self.events();
        let wait = async {
            let current = self.connection_status().await?;
            if current != ConnectionStatus::Disconnected {
                return Ok(current);
            }
            loop {
                match events.recv().await {
                    Ok(DeliveryEvent::ConnectionStatusChange { connection_status })
                        if connection_status != ConnectionStatus::Disconnected =>
                    {
                        return Ok(connection_status)
                    }
                    Ok(_) | Err(broadcast::error::RecvError::Lagged(_)) => {}
                    Err(broadcast::error::RecvError::Closed) => {
                        return Err(DeliveryError::Timeout("connection"))
                    }
                }
            }
        };
        tokio::time::timeout(timeout, wait)
            .await
            .map_err(|_| DeliveryError::Timeout("connection"))?
    }

    /// Addresses other nodes can dial this one on, as multiaddrs.
    pub async fn listen_addresses(&self) -> Result<Vec<String>> {
        let addresses = self
            .inner
            .ctx()
            .waku_listen_addresses_async()
            .await
            .map_err(DeliveryError::Startup)?;
        Ok(addresses
            .trim_matches('"')
            .split(',')
            .filter(|a| !a.is_empty())
            .map(str::to_string)
            .collect())
    }

    /// Dials the peer at `multiaddr`.
    pub async fn connect(&self, multiaddr: &str, timeout: Duration) -> Result<()> {
        self.inner
            .ctx()
            .waku_connect_async(multiaddr.to_string(), timeout.as_millis() as u32)
            .await
            .map(|_| ())
            .map_err(DeliveryError::Startup)
    }

    /// Opens a reliable channel on this node.
    pub async fn create_channel(&self, config: ChannelConfig) -> Result<Channel> {
        Channel::create(self.clone(), config).await
    }

    /// Stops the node. It is destroyed once the last clone is dropped.
    pub async fn shutdown(&self) -> Result<()> {
        self.inner
            .ctx()
            .stop_node_async()
            .await
            .map(|_| ())
            .map_err(DeliveryError::Shutdown)
    }

    pub(crate) fn ctx(&self) -> &LogosDeliveryCtx {
        self.inner.ctx()
    }
}
