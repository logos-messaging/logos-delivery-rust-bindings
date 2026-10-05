use base64::Engine;
use serde::{Deserialize, Deserializer};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum ConnectionStatus {
    Disconnected,
    PartiallyConnected,
    Connected,
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
    #[serde(default)]
    pub timestamp: i64,
    #[serde(default)]
    pub ephemeral: bool,
}

/// Everything the node reports asynchronously. Unknown event types are dropped.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "eventType", rename_all = "snake_case")]
pub enum DeliveryEvent {
    #[serde(rename_all = "camelCase")]
    MessageQueued {
        request_id: String,
        message_hash: String,
    },
    #[serde(rename_all = "camelCase")]
    MessageSent {
        request_id: String,
        message_hash: String,
    },
    #[serde(rename_all = "camelCase")]
    MessagePropagated {
        request_id: String,
        message_hash: String,
    },
    #[serde(rename_all = "camelCase")]
    MessageError {
        request_id: String,
        message_hash: String,
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
    "onMessageQueued",
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
    use super::*;

    #[test]
    fn parses_message_received() {
        let json = r#"{"eventType":"message_received","messageHash":"0xab","source":"live",
            "message":{"payload":"aGk=","contentTopic":"/a/1/b/proto","version":0,
            "timestamp":1,"ephemeral":false,"meta":"","proof":""}}"#;
        match serde_json::from_str::<DeliveryEvent>(json).unwrap() {
            DeliveryEvent::MessageReceived {
                message, source, ..
            } => {
                assert_eq!(message.payload, b"hi");
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
    fn unknown_event_is_an_error_not_a_panic() {
        assert!(serde_json::from_str::<DeliveryEvent>(r#"{"eventType":"nope"}"#).is_err());
    }
}
