//! Async (tokio) client for Logos Delivery, including reliable channels.
//!
//! ```no_run
//! # async fn demo() -> logos_delivery::Result<()> {
//! use logos_delivery::{ChannelConfig, DeliveryConfig, DeliveryNode};
//! use tokio_stream::StreamExt;
//!
//! let node = DeliveryNode::start(DeliveryConfig::default().tcp_port(60000)).await?;
//! node.wait_connected(std::time::Duration::from_secs(30)).await?;
//!
//! let channel = node
//!     .create_channel(ChannelConfig {
//!         channel_id: "chat".into(),
//!         content_topic: "/my-app/1/chat/proto".into(),
//!         sender_id: "alice".into(),
//!     })
//!     .await?;
//! channel.send(b"hello").await?;
//! let mut events = Box::pin(channel.events());
//! while let Some(event) = events.next().await {
//!     println!("{event:?}");
//! }
//! node.shutdown().await
//! # }
//! ```
//!
//! The FFI layer in `generated/` is emitted by nim-ffi; regenerate it with
//! `scripts/gen-bindings.sh`.

mod channel;
mod config;
mod error;
mod events;
#[rustfmt::skip]
mod generated;
mod node;

pub use channel::{Channel, ChannelConfig, ChannelEvent};
pub use config::DeliveryConfig;
pub use error::{DeliveryError, Result};
pub use events::{ConnectionStatus, DeliveryEvent, MessageSource, ReceivedMessage};
pub use node::{DeliveryNode, RequestId};
