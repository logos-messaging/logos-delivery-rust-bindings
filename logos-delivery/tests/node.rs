use std::time::Duration;

use logos_delivery::{
    ChannelConfig, ConnectionStatus, DeliveryConfig, DeliveryEvent, DeliveryNode,
};
use serial_test::serial;
use tokio_stream::StreamExt;

const TOPIC: &str = "/logos-delivery-test/1/chat/proto";
const TIMEOUT: Duration = Duration::from_secs(30);

/// Ports stay below 32768, the start of Linux's ephemeral range, so the runner's own
/// outgoing connections cannot hold one. (Same in blocking.rs and version_check.rs.)
/// Autosharding needs a shard count, and a node must sit on every shard a content
/// topic may hash to. Persistency is a process-wide singleton, so nodes in one
/// binary share a root.
fn config(port: u16) -> DeliveryConfig {
    DeliveryConfig::default()
        .no_preset()
        .tcp_port(port)
        .discv5_udp_port(port + 1000)
        .option("clusterId", 99)
        .option("numShardsInNetwork", 8)
        .option("shards", (0..8).collect::<Vec<_>>())
        .option("localStoragePath", "./data-logos-delivery-test")
}

#[tokio::test]
#[serial]
async fn node_starts_and_stops() {
    let node = DeliveryNode::start(config(20110)).await.expect("start");
    assert_eq!(
        node.connection_status().await.expect("status"),
        ConnectionStatus::Disconnected,
        "a lone node has no peers"
    );
    node.subscribe(TOPIC).await.expect("subscribe");
    node.unsubscribe(TOPIC).await.expect("unsubscribe");
    node.shutdown().await.expect("shutdown");
}

#[tokio::test]
#[serial]
async fn channel_create_send_close() {
    let node = DeliveryNode::start(config(20120)).await.expect("start");
    let channel = node
        .create_channel(ChannelConfig {
            channel_id: "test-channel".into(),
            content_topic: TOPIC.into(),
            sender_id: "alice".into(),
        })
        .await
        .expect("create channel");

    assert!(channel.exists().await.expect("exists"));
    let request_id = channel.send(b"hello").await.expect("send");
    assert!(!request_id.0.is_empty());
    channel.close().await.expect("close");
    assert!(!channel.exists().await.expect("exists after close"));
    node.shutdown().await.expect("shutdown");
}

#[tokio::test]
#[serial]
async fn published_message_reaches_peer() {
    let sender = DeliveryNode::start(config(20130)).await.expect("sender");
    let receiver = DeliveryNode::start(config(20140)).await.expect("receiver");

    let address = receiver.listen_addresses().await.expect("addresses")[0].clone();
    sender.connect(&address, TIMEOUT).await.expect("connect");
    sender
        .wait_connected(TIMEOUT)
        .await
        .expect("sender connected");

    receiver.subscribe(TOPIC).await.expect("subscribe");
    sender.subscribe(TOPIC).await.expect("subscribe");
    let mut messages = Box::pin(receiver.messages());
    let mut sender_events = sender.events();
    // Let the mesh form before publishing.
    tokio::time::sleep(Duration::from_secs(5)).await;

    let request_id = sender.publish(TOPIC, b"ping").await.expect("publish");

    let message = tokio::time::timeout(TIMEOUT, messages.next())
        .await
        .expect("message in time")
        .expect("stream open");
    assert_eq!(message.payload, b"ping");
    assert_eq!(message.content_topic, TOPIC);

    // The sender reports the fate of the request it was handed.
    let outcome = tokio::time::timeout(TIMEOUT, async {
        loop {
            match sender_events.recv().await {
                Ok(DeliveryEvent::MessageSent { request_id: id, .. })
                | Ok(DeliveryEvent::MessagePropagated { request_id: id, .. })
                    if id == request_id.0 =>
                {
                    break;
                }
                Ok(_) => {}
                Err(e) => panic!("event stream ended: {e}"),
            }
        }
    })
    .await;
    assert!(
        outcome.is_ok(),
        "sender should see its request sent or propagated"
    );

    sender.shutdown().await.expect("shutdown");
    receiver.shutdown().await.expect("shutdown");
}

#[tokio::test]
#[serial]
async fn start_waits_for_a_peer_but_does_not_fail_without_one() {
    let started = std::time::Instant::now();
    let node = DeliveryNode::start(config(20150).wait_for_connection(Duration::from_secs(2)))
        .await
        .expect("a lone node still starts");
    assert!(
        started.elapsed() >= Duration::from_secs(2),
        "start should have waited out the connection timeout"
    );
    node.shutdown().await.expect("shutdown");
}

/// A node that is still disconnected when `wait_connected` starts has to learn about
/// the connection from the `connection_status_change` event.
#[tokio::test]
#[serial]
async fn wait_connected_returns_on_the_connection_event() {
    let sender = DeliveryNode::start(config(20160)).await.expect("sender");
    let receiver = DeliveryNode::start(config(20170)).await.expect("receiver");
    assert_eq!(
        sender.connection_status().await.expect("status"),
        ConnectionStatus::Disconnected
    );

    let waiter = tokio::spawn({
        let sender = sender.clone();
        async move { sender.wait_connected(TIMEOUT).await }
    });
    // Let the waiter read the (disconnected) status and subscribe to events.
    tokio::time::sleep(Duration::from_secs(1)).await;
    let address = receiver.listen_addresses().await.expect("addresses")[0].clone();
    sender.connect(&address, TIMEOUT).await.expect("connect");

    let status = waiter.await.expect("task").expect("connected");
    assert_ne!(status, ConnectionStatus::Disconnected);

    sender.shutdown().await.expect("shutdown");
    receiver.shutdown().await.expect("shutdown");
}

#[tokio::test]
#[serial]
async fn channel_created_with_its_event_stream() {
    let node = DeliveryNode::start(config(20180)).await.expect("node");
    let (channel, events) = node
        .create_channel_with_events(ChannelConfig {
            channel_id: "stream-channel".into(),
            content_topic: TOPIC.into(),
            sender_id: "alice".into(),
        })
        .await
        .expect("create channel");
    let _events = Box::pin(events);
    assert!(channel.exists().await.expect("exists"));
    channel.close().await.expect("close");
    node.shutdown().await.expect("shutdown");
}
