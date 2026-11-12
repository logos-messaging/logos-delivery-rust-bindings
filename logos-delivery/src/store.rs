use serde::{Deserialize, Serialize};

use crate::events::ReceivedMessage;

/// The protocol id a peer that answers [`DeliveryNode::store_query`](crate::DeliveryNode::store_query)
/// lists in [`PeerInfo::protocols`](crate::PeerInfo::protocols).
pub const STORE_CODEC: &str = "/vac/waku/store-query/3.0.0";

/// Which messages [`DeliveryNode::store_query`](crate::DeliveryNode::store_query) asks a
/// store peer for. A filter left unset matches every message.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StoreQuery {
    /// The reply carries each message, not only its hash.
    pub include_data: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pubsub_topic: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub content_topics: Vec<String>,
    /// Unix time in nanoseconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time_start: Option<i64>,
    /// Unix time in nanoseconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time_end: Option<i64>,
    /// 0x-prefixed hex, as [`DeliveryEvent`](crate::DeliveryEvent)s carry them.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub message_hashes: Vec<String>,
    /// The previous page's [`StoreResponse::pagination_cursor`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pagination_cursor: Option<String>,
    /// Pages run oldest first.
    pub pagination_forward: bool,
    /// Messages per page; the peer's default when unset.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pagination_limit: Option<u64>,
}

impl StoreQuery {
    /// The JSON `waku_store_query` takes. The store client fills in the empty request id.
    pub(crate) fn to_json(&self) -> String {
        let mut json = serde_json::to_value(self).expect("a store query serializes");
        json["requestId"] = "".into();
        json.to_string()
    }
}

/// One page of the messages a [`StoreQuery`] matched.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoreResponse {
    pub messages: Vec<StoredMessage>,
    /// Set while more pages follow.
    pub pagination_cursor: Option<String>,
}

/// A message a store peer keeps.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredMessage {
    /// 0x-prefixed hex, as [`DeliveryEvent`](crate::DeliveryEvent)s carry them.
    pub message_hash: String,
    /// Set when the query set [`StoreQuery::include_data`].
    pub message: Option<ReceivedMessage>,
    pub pubsub_topic: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_holds_the_keys_the_kernel_requires_and_no_unset_filter() {
        let query = StoreQuery {
            include_data: true,
            message_hashes: vec!["0xab".into()],
            ..Default::default()
        };
        let json: serde_json::Value = serde_json::from_str(&query.to_json()).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "requestId": "",
                "includeData": true,
                "messageHashes": ["0xab"],
                "paginationForward": false,
            })
        );
    }

    #[test]
    fn parses_a_store_response() {
        let json = r#"{"requestId":"r","statusCode":200,"statusDesc":"OK","messages":[
            {"messageHash":"0xab","pubsubTopic":"/waku/2/rs/99/0","message":{"payload":"aGk=",
                "contentTopic":"/a/1/b/proto","version":0,"timestamp":1,"ephemeral":false}},
            {"messageHash":"0xcd"}],"paginationCursor":"0xcd"}"#;
        let response: StoreResponse = serde_json::from_str(json).unwrap();
        assert_eq!(
            response.messages[0].message.as_ref().unwrap().payload,
            b"hi"
        );
        assert!(response.messages[1].message.is_none());
        assert_eq!(response.pagination_cursor.as_deref(), Some("0xcd"));
    }
}
