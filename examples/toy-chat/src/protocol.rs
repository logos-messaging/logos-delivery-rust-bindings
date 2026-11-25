use chrono::{DateTime, LocalResult, TimeZone, Utc};
use prost::Message;
use waku::{Encoding, WakuContentTopic};

pub static TOY_CHAT_CONTENT_TOPIC: WakuContentTopic =
    WakuContentTopic::new("toy-chat", "2", "huilong", Encoding::Proto);

#[derive(Clone, Message)]
pub struct Chat2Message {
    #[prost(uint64, tag = "1")]
    timestamp: u64,
    #[prost(string, tag = "2")]
    nick: String,
    #[prost(bytes, tag = "3")]
    payload: Vec<u8>,
}

impl Chat2Message {
    pub fn new(nick: &str, payload: &str) -> Self {
        Self {
            timestamp: Utc::now().timestamp() as u64,
            nick: nick.to_string(),
            payload: payload.as_bytes().to_vec(),
        }
    }
    pub fn message(&self) -> String {
        String::from_utf8(self.payload.clone()).unwrap()
    }

    pub fn nick(&self) -> &str {
        &self.nick
    }

    pub fn timestamp(&self) -> LocalResult<DateTime<Utc>> {
        Utc.timestamp_opt(self.timestamp as i64, 0)
    }
}

mod tests {
    use crate::{Chat2Message, Multiaddr, NODES};
    use prost::Message;
    use std::str::FromStr;
    use waku::{
        waku_new, waku_set_event_callback, ContentFilter, Encoding, Event, PagingOptions,
        ProtocolId, Result, StoreQuery, WakuContentTopic, WakuMessage, WakuNodeConfig,
    };

    #[ignore]
    #[test]
    fn main() -> Result<()> {
        let chat2_content_topic = WakuContentTopic {
            application_name: "toy-chat".into(),
            version: 2,
            content_topic_name: "huilong".into(),
            encoding: Encoding::Proto,
        };

        let handle = waku_new(None)?.start()?;

        let mut peer_id = None;
        for address in NODES {
            peer_id =
                Some(handle.add_peer(&Multiaddr::from_str(*address).unwrap(), ProtocolId::Store)?);
            handle.connect_peer_with_id(peer_id.as_ref().unwrap().clone(), None)?;
        }
        let peer_id = peer_id.unwrap();

        let response = handle.store_query(
            &StoreQuery {
                pubsub_topic: None,
                content_filters: vec![ContentFilter::new(chat2_content_topic)],
                start_time: None,
                end_time: None,
                paging_options: Some(PagingOptions {
                    page_size: 100,
                    cursor: None,
                    forward: true,
                }),
            },
            &peer_id,
            None,
        )?;

        for message in response
            .messages()
            .iter()
            .map(|message| Chat2Message::decode(message.payload()).unwrap())
        {
            println!(
                "[{} - {}]: {}",
                message.timestamp(),
                message.nick(),
                message.message(),
            )
        }
        Ok(())
    }
}
