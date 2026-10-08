//! Blocking facade over [`DeliveryNode`] for synchronous callers.
//!
//! [`BlockingDeliveryNode`] bridges every call with `Handle::block_on` on a tokio
//! runtime the caller owns and passes in, so this crate never creates one. Like
//! `Handle::block_on`, its methods panic when called from inside a tokio runtime,
//! and so does dropping the last clone there: use [`DeliveryNode`] directly in
//! async code. The runtime must have its time driver enabled (`enable_time` or
//! `enable_all`), and be multi-threaded or driven by another thread
//! (`Runtime::block_on`), for background tasks such as [`BlockingDeliveryNode::events`]
//! to make progress.

use std::sync::Arc;
use std::time::Duration;

use crossbeam_channel::{bounded, Receiver, TrySendError};
use futures_core::Stream;
use tokio::runtime::Handle;
use tokio_stream::StreamExt;

use crate::channel::ChannelConfig;
use crate::config::DeliveryConfig;
use crate::error::Result;
use crate::events::{ConnectionStatus, DeliveryEvent, ReceivedMessage};
use crate::node::{DeliveryNode, RequestId};
use crate::Channel;

/// Items buffered for a consumer before new ones are dropped.
const QUEUE_CAPACITY: usize = 1024;

struct Shared {
    node: DeliveryNode,
    runtime: Handle,
}

/// A [`DeliveryNode`] driven synchronously. Cheap to clone; clones share the node,
/// which stops once the last clone (and channel handle) is dropped.
#[derive(Clone)]
pub struct BlockingDeliveryNode {
    shared: Arc<Shared>,
}

impl BlockingDeliveryNode {
    /// Starts a node, driving it on the runtime behind `runtime`. The caller owns
    /// that runtime and must keep it alive while the node is in use. Returns once
    /// the node is started, not once it is connected: see
    /// [`BlockingDeliveryNode::wait_connected`].
    pub fn start(config: DeliveryConfig, runtime: Handle) -> Result<Self> {
        let node = runtime.block_on(DeliveryNode::start(config))?;
        Ok(Self {
            shared: Arc::new(Shared { node, runtime }),
        })
    }

    pub fn subscribe(&self, content_topic: &str) -> Result<()> {
        self.block_on(self.shared.node.subscribe(content_topic))
    }

    pub fn unsubscribe(&self, content_topic: &str) -> Result<()> {
        self.block_on(self.shared.node.unsubscribe(content_topic))
    }

    pub fn publish(&self, content_topic: &str, payload: &[u8]) -> Result<RequestId> {
        self.block_on(self.shared.node.publish(content_topic, payload))
    }

    pub fn connection_status(&self) -> Result<ConnectionStatus> {
        self.block_on(self.shared.node.connection_status())
    }

    /// Waits until the node reports at least one connection.
    pub fn wait_connected(&self, timeout: Duration) -> Result<ConnectionStatus> {
        self.block_on(self.shared.node.wait_connected(timeout))
    }

    /// Addresses other nodes can dial this one on, as multiaddrs.
    pub fn listen_addresses(&self) -> Result<Vec<String>> {
        self.block_on(self.shared.node.listen_addresses())
    }

    /// Dials the peer at `multiaddr`.
    pub fn connect(&self, multiaddr: &str, timeout: Duration) -> Result<()> {
        self.block_on(self.shared.node.connect(multiaddr, timeout))
    }

    /// Stops the node. Clones share it, so this ends delivery for all of them.
    pub fn shutdown(&self) -> Result<()> {
        self.block_on(self.shared.node.shutdown())
    }

    pub fn create_channel(&self, config: ChannelConfig) -> Result<BlockingChannel> {
        let channel = self.block_on(self.shared.node.create_channel(config))?;
        Ok(BlockingChannel {
            shared: self.shared.clone(),
            channel,
        })
    }

    /// Received messages passed through `map`, which keeps the `Some` results.
    ///
    /// The queue is bounded: items arriving while it is full are dropped with a
    /// warning. `map` runs on the runtime, never on the node's event thread. The
    /// forwarder ends when the receiver is dropped. Only messages received after
    /// this call are seen.
    pub fn inbound_queue<T: Send + 'static>(
        &self,
        map: impl FnMut(ReceivedMessage) -> Option<T> + Send + 'static,
    ) -> Receiver<T> {
        self.forward(self.shared.node.messages(), map)
    }

    /// Every node event, with the same queue semantics as [`Self::inbound_queue`].
    pub fn events(&self) -> Receiver<DeliveryEvent> {
        self.forward(
            tokio_stream::wrappers::BroadcastStream::new(self.shared.node.events()).filter_map(
                |event| match event {
                    Ok(event) => Some(event),
                    Err(e) => {
                        tracing::warn!("event stream lagged: {e}");
                        None
                    }
                },
            ),
            Some,
        )
    }

    fn forward<S, I, T>(
        &self,
        stream: S,
        mut map: impl FnMut(I) -> Option<T> + Send + 'static,
    ) -> Receiver<T>
    where
        S: Stream<Item = I> + Send + 'static,
        I: Send + 'static,
        T: Send + 'static,
    {
        let (tx, rx) = bounded(QUEUE_CAPACITY);
        // The stream exists before the task starts, so nothing after this call is missed.
        let mut stream = Box::pin(stream);
        self.shared.runtime.spawn(async move {
            while let Some(item) = stream.next().await {
                let Some(mapped) = map(item) else { continue };
                match tx.try_send(mapped) {
                    Ok(()) => {}
                    Err(TrySendError::Full(_)) => {
                        tracing::warn!("inbound queue full, dropping item")
                    }
                    Err(TrySendError::Disconnected(_)) => break,
                }
            }
        });
        rx
    }

    fn block_on<T>(&self, future: impl std::future::Future<Output = T>) -> T {
        self.shared.runtime.block_on(future)
    }
}

/// A [`Channel`] driven synchronously. Dropping the handle leaves the channel
/// open; call [`BlockingChannel::close`] to end it.
#[derive(Clone)]
pub struct BlockingChannel {
    shared: Arc<Shared>,
    channel: Channel,
}

impl BlockingChannel {
    pub fn id(&self) -> &str {
        self.channel.id()
    }

    pub fn send(&self, payload: &[u8]) -> Result<RequestId> {
        self.shared.runtime.block_on(self.channel.send(payload))
    }

    pub fn exists(&self) -> Result<bool> {
        self.shared.runtime.block_on(self.channel.exists())
    }

    pub fn close(&self) -> Result<()> {
        self.shared.runtime.block_on(self.channel.close())
    }
}
