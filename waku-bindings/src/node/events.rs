//! Waku message [event](https://rfc.vac.dev/spec/36/#events) related items
//!
//! Asynchronous events require a callback to be registered.
//! An example of an asynchronous event that might be emitted is receiving a message.
//! When an event is emitted, this callback will be triggered receiving an [`WakuEvent`]

// crates
use serde::{Deserialize, Serialize};
// internal
use crate::general::WakuMessage;
use std::str;

use crate::MessageHash;

/// Waku event
/// For now just WakuMessage is supported
#[non_exhaustive]
#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "eventType", rename_all = "camelCase")]
pub enum WakuEvent {
    #[serde(rename = "message")]
    WakuMessage(WakuMessageEvent),

    #[serde(rename = "relay_topic_health_change")]
    RelayTopicHealthChange(TopicHealthEvent),

    #[serde(rename = "connection_change")]
    ConnectionChange(ConnectionChangeEvent),

    #[serde(rename = "node_health_change")]
    NodeHealthChange(NodeHealthChangeEvent),

    Unrecognized(serde_json::Value),
}

/// Type of `event` field for a `message` event
#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct WakuMessageEvent {
    /// The pubsub topic on which the message was received
    pub pubsub_topic: String,
    /// The message hash
    pub message_hash: MessageHash,
    /// The message in [`WakuMessage`] format
    pub waku_message: WakuMessage,
}

/// Type of `event` field for a `topic health` event
#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct TopicHealthEvent {
    /// The pubsub topic on which the message was received
    pub pubsub_topic: String,
    /// The message hash
    pub topic_health: String,
}

/// Type of `event` field for a `connection change` event
#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionChangeEvent {
    pub peer_id: String,
    pub peer_event: String,
}

/// Type of `event` field for a `node_health_change` event
#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct NodeHealthChangeEvent {
    pub status: String,
}

#[cfg(test)]
mod tests {
    use crate::WakuEvent;
    use crate::WakuEvent::{ConnectionChange, RelayTopicHealthChange};

    #[test]
    fn deserialize_message_event() {
        let s = "{\"eventType\":\"message\",\"messageHash\":\"0xd40aa51bbb4867fe40329a255575cfc9ef4000358cc7321b2668b008cba94b30\",\"pubsubTopic\":\"/waku/2/default-waku/proto\",\"wakuMessage\":{\"payload\":\"SGkgZnJvbSDwn6aAIQ==\",\"contentTopic\":\"/toychat/2/huilong/proto\",\"timestamp\":1665580926660}}";
        let evt: WakuEvent = serde_json::from_str(s).unwrap();
        assert!(matches!(evt, WakuEvent::WakuMessage(_)));
    }

    #[test]
    fn deserialize_topic_health_change_event() {
        let s = "{\"eventType\":\"relay_topic_health_change\", \"pubsubTopic\":\"/waku/2/rs/16/1\",\"topicHealth\":\"MinimallyHealthy\"}";
        let evt: WakuEvent = serde_json::from_str(s).unwrap();
        match evt {
            RelayTopicHealthChange(topic_health_event) => {
                assert_eq!(topic_health_event.pubsub_topic, "/waku/2/rs/16/1");
                assert_eq!(topic_health_event.topic_health, "MinimallyHealthy");
            }
            _ => panic!("Expected RelayTopicHealthChange event, but got {:?}", evt),
        }
    }

    #[test]
    fn deserialize_connection_change_event() {
        let s = "{\"eventType\":\"connection_change\", \"peerId\":\"16Uiu2HAmAR24Mbb6VuzoyUiGx42UenDkshENVDj4qnmmbabLvo31\",\"peerEvent\":\"Joined\"}";
        let evt: WakuEvent = serde_json::from_str(s).unwrap();
        match evt {
            ConnectionChange(conn_change_event) => {
                assert_eq!(
                    conn_change_event.peer_id,
                    "16Uiu2HAmAR24Mbb6VuzoyUiGx42UenDkshENVDj4qnmmbabLvo31"
                );
                assert_eq!(conn_change_event.peer_event, "Joined");
            }
            _ => panic!("Expected RelayTopicHealthChange event, but got {:?}", evt),
        }
    }
}
