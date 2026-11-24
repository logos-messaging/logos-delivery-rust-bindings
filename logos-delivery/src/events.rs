use base64::Engine;
use serde::{Deserialize, Deserializer};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum ConnectionStatus {
    Disconnected,
    PartiallyConnected,
    Connected,
}

/// Why a message that `publish` accepted failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum MessageErrorKind {
    /// No peer took the message within the delivery window.
    NotPropagated,
    /// The message propagated, but no store node reported it within its window.
    NotStored,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MessageSource {
    Live,
    History,
}

/// A message delivered by the node (not through a reliable channel).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReceivedMessage {
    #[serde(deserialize_with = "base64_bytes")]
    pub payload: Vec<u8>,
    pub content_topic: String,
    #[serde(default, deserialize_with = "base64_bytes")]
    pub meta: Vec<u8>,
    #[serde(default)]
    pub version: u32,
    #[serde(default)]
    pub timestamp: i64,
    #[serde(default)]
    pub ephemeral: bool,
    #[serde(default, deserialize_with = "base64_bytes")]
    pub proof: Vec<u8>,
}

/// Everything the node reports asynchronously. Unknown event types are dropped.
///
/// The send events carry the message hash that `publish` returned.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "eventType", rename_all = "snake_case")]
pub enum DeliveryEvent {
    /// A store node, or the mix exit for a send over mix, confirmed the message.
    #[serde(rename_all = "camelCase")]
    MessageSent { message_hash: String },
    /// The message reached at least one peer.
    #[serde(rename_all = "camelCase")]
    MessagePropagated { message_hash: String },
    #[serde(rename_all = "camelCase")]
    MessageError {
        message_hash: String,
        kind: MessageErrorKind,
        error: String,
    },
    #[serde(rename_all = "camelCase")]
    MessageReceived {
        message_hash: String,
        message: ReceivedMessage,
        source: MessageSource,
    },
    #[serde(rename_all = "camelCase")]
    ConnectionStatusChange { connection_status: ConnectionStatus },
    #[serde(rename_all = "camelCase")]
    RelayTopicHealthChange {
        pubsub_topic: String,
        topic_health: String,
    },
    #[serde(rename_all = "camelCase")]
    ConnectionChange { peer_id: String, peer_event: String },
    #[serde(rename_all = "camelCase")]
    ChannelMessageReceived {
        channel_id: String,
        sender_id: String,
        #[serde(deserialize_with = "base64_bytes")]
        payload: Vec<u8>,
    },
    #[serde(rename_all = "camelCase")]
    ChannelMessageSent {
        channel_id: String,
        request_id: String,
    },
    #[serde(rename_all = "camelCase")]
    ChannelMessageError {
        channel_id: String,
        request_id: String,
        error: String,
    },
    #[serde(rename_all = "camelCase")]
    ChannelMessageLost {
        channel_id: String,
        payload_hash: String,
        reason: String,
    },
}

fn base64_bytes<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
    let s = String::deserialize(d)?;
    base64::engine::general_purpose::STANDARD
        .decode(s)
        .map_err(serde::de::Error::custom)
}

/// Names under which the library publishes its events.
pub(crate) const EVENT_NAMES: &[&str] = &[
    "onMessageSent",
    "onMessagePropagated",
    "onMessageError",
    "onMessageReceived",
    "onConnectionStatusChange",
    "onTopicHealthChange",
    "onConnectionChange",
    "onChannelMessageReceived",
    "onChannelMessageSent",
    "onChannelMessageError",
    "onChannelMessageLost",
];

#[cfg(test)]
mod tests {
    use base64::engine::general_purpose::STANDARD;
    use serde_json::json;

    use super::*;

    #[test]
    fn parses_message_received() {
        let json = json!({"eventType": "message_received", "messageHash": "0xab", "source": "live",
            "message": {"payload": STANDARD.encode(b"hi"), "contentTopic": "/a/1/b/proto",
                "version": 0, "timestamp": 1, "ephemeral": false,
                "meta": STANDARD.encode(b"meta"), "proof": ""}});
        match serde_json::from_value::<DeliveryEvent>(json).unwrap() {
            DeliveryEvent::MessageReceived {
                message, source, ..
            } => {
                assert_eq!(message.payload, b"hi");
                assert_eq!(message.meta, b"meta");
                assert_eq!(source, MessageSource::Live);
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn parses_channel_events() {
        let json = r#"{"eventType":"channel_message_received","channelId":"c","senderId":"s","payload":"aGk="}"#;
        assert!(matches!(
            serde_json::from_str::<DeliveryEvent>(json).unwrap(),
            DeliveryEvent::ChannelMessageReceived { payload, .. } if payload == b"hi"
        ));
        let json = r#"{"eventType":"channel_message_lost","channelId":"c","payloadHash":"ab","reason":"expired"}"#;
        assert!(matches!(
            serde_json::from_str::<DeliveryEvent>(json).unwrap(),
            DeliveryEvent::ChannelMessageLost { .. }
        ));
    }

    #[test]
    fn parses_send_events() {
        let json = r#"{"eventType":"message_propagated","messageHash":"0xab"}"#;
        assert!(matches!(
            serde_json::from_str::<DeliveryEvent>(json).unwrap(),
            DeliveryEvent::MessagePropagated { message_hash } if message_hash == "0xab"
        ));
        let json =
            r#"{"eventType":"message_error","messageHash":"0xab","kind":"NotStored","error":"e"}"#;
        assert!(matches!(
            serde_json::from_str::<DeliveryEvent>(json).unwrap(),
            DeliveryEvent::MessageError {
                kind: MessageErrorKind::NotStored,
                ..
            }
        ));
    }

    #[test]
    fn unknown_event_is_an_error_not_a_panic() {
        assert!(serde_json::from_str::<DeliveryEvent>(r#"{"eventType":"nope"}"#).is_err());
    }
}
