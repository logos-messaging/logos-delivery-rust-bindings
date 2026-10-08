use base64::Engine;
use futures_core::Stream;
use serde_json::json;
use tokio_stream::{wrappers::BroadcastStream, StreamExt};

use crate::error::{DeliveryError, Result};
use crate::events::DeliveryEvent;
use crate::node::{DeliveryNode, RequestId};

#[derive(Debug, Clone)]
pub struct ChannelConfig {
    pub channel_id: String,
    pub content_topic: String,
    pub sender_id: String,
}

/// What happened on a channel.
#[derive(Debug, Clone)]
pub enum ChannelEvent {
    Received {
        sender_id: String,
        payload: Vec<u8>,
    },
    /// Every segment of the send was confirmed.
    Sent {
        request_id: RequestId,
    },
    Error {
        request_id: RequestId,
        error: String,
    },
    /// A message was given up on (expired, evicted or failed integrity checks).
    Lost {
        payload_hash: String,
        reason: String,
    },
}

/// A reliable channel: segmented, acknowledged and retried by the node. Dropping
/// the handle leaves the channel open; call [`Channel::close`] to end it.
#[derive(Clone)]
pub struct Channel {
    node: DeliveryNode,
    channel_id: String,
}

impl Channel {
    pub(crate) async fn create(node: DeliveryNode, config: ChannelConfig) -> Result<Self> {
        // All three zero: no encryption.
        node.ctx()
            .channel_create_async(
                config.channel_id.clone(),
                config.content_topic,
                config.sender_id,
                0,
                0,
                0,
            )
            .await
            .map_err(DeliveryError::Channel)?;
        Ok(Self {
            node,
            channel_id: config.channel_id,
        })
    }

    pub fn id(&self) -> &str {
        &self.channel_id
    }

    pub async fn send(&self, payload: &[u8]) -> Result<RequestId> {
        self.send_message(payload, false).await
    }

    /// Like [`Channel::send`], for a message that store nodes should not keep.
    pub async fn send_ephemeral(&self, payload: &[u8]) -> Result<RequestId> {
        self.send_message(payload, true).await
    }

    async fn send_message(&self, payload: &[u8], ephemeral: bool) -> Result<RequestId> {
        let message = json!({
            "payload": base64::engine::general_purpose::STANDARD.encode(payload),
            "ephemeral": ephemeral,
        });
        self.node
            .ctx()
            .channel_send_async(self.channel_id.clone(), message.to_string())
            .await
            .map(RequestId)
            .map_err(DeliveryError::Channel)
    }

    pub async fn exists(&self) -> Result<bool> {
        self.node
            .ctx()
            .channel_exists_async(self.channel_id.clone())
            .await
            .map(|reply| reply.trim_matches('"') == "true")
            .map_err(DeliveryError::Channel)
    }

    pub async fn close(&self) -> Result<()> {
        self.node
            .ctx()
            .channel_close_async(self.channel_id.clone())
            .await
            .map(|_| ())
            .map_err(DeliveryError::Channel)
    }

    /// Events of this channel only, from now on.
    pub fn events(&self) -> impl Stream<Item = ChannelEvent> {
        let id = self.channel_id.clone();
        BroadcastStream::new(self.node.events()).filter_map(move |event| match event {
            Ok(DeliveryEvent::ChannelMessageReceived {
                channel_id,
                sender_id,
                payload,
            }) if channel_id == id => Some(ChannelEvent::Received { sender_id, payload }),
            Ok(DeliveryEvent::ChannelMessageSent {
                channel_id,
                request_id,
            }) if channel_id == id => Some(ChannelEvent::Sent {
                request_id: RequestId(request_id),
            }),
            Ok(DeliveryEvent::ChannelMessageError {
                channel_id,
                request_id,
                error,
            }) if channel_id == id => Some(ChannelEvent::Error {
                request_id: RequestId(request_id),
                error,
            }),
            Ok(DeliveryEvent::ChannelMessageLost {
                channel_id,
                payload_hash,
                reason,
            }) if channel_id == id => Some(ChannelEvent::Lost {
                payload_hash,
                reason,
            }),
            Ok(_) => None,
            Err(e) => {
                tracing::warn!("channel event stream lagged: {e}");
                None
            }
        })
    }
}
