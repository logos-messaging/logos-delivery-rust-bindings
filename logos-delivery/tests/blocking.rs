use std::time::Duration;

use crossbeam_channel::RecvTimeoutError;
use logos_delivery::blocking::BlockingDeliveryNode;
use logos_delivery::{ChannelConfig, DeliveryConfig, DeliveryEvent};

const TOPIC: &str = "/logos-delivery-blocking-test/1/chat/proto";
const TIMEOUT: Duration = Duration::from_secs(30);

/// Persistency is a process-wide singleton, so both nodes share a root.
fn config(port: u16) -> DeliveryConfig {
    DeliveryConfig::default()
        .no_preset()
        .tcp_port(port)
        .discv5_udp_port(port + 1000)
        .option("clusterId", 99)
        .option("numShardsInNetwork", 8)
        .option("shards", (0..8).collect::<Vec<_>>())
        .option("localStoragePath", "./data-logos-delivery-blocking-test")
}

#[test]
fn published_message_reaches_peer_through_inbound_queue() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .expect("runtime");
    let sender =
        BlockingDeliveryNode::start(config(24010), runtime.handle().clone()).expect("sender");
    let receiver =
        BlockingDeliveryNode::start(config(24020), runtime.handle().clone()).expect("receiver");

    let address = receiver.listen_addresses().expect("addresses")[0].clone();
    sender.connect(&address, TIMEOUT).expect("connect");
    sender.wait_connected(TIMEOUT).expect("sender connected");

    receiver.subscribe(TOPIC).expect("subscribe");
    sender.subscribe(TOPIC).expect("subscribe");
    // Messages on other topics are filtered out by the mapping.
    let inbound = receiver.inbound_queue(|m| (m.content_topic == TOPIC).then_some(m.payload));
    let events = receiver.events();
    // Let the mesh form before publishing.
    std::thread::sleep(Duration::from_secs(5));

    let request_id = sender.publish(TOPIC, b"ping").expect("publish");
    assert!(!request_id.0.is_empty());

    match inbound.recv_timeout(TIMEOUT) {
        Ok(payload) => assert_eq!(payload, b"ping"),
        Err(RecvTimeoutError::Timeout) => panic!("message not received in time"),
        Err(RecvTimeoutError::Disconnected) => panic!("forwarder ended early"),
    }

    // The same message also arrives as a typed event.
    loop {
        match events.recv_timeout(TIMEOUT).expect("event in time") {
            DeliveryEvent::MessageReceived { message, .. } if message.content_topic == TOPIC => {
                assert_eq!(message.payload, b"ping");
                break;
            }
            _ => {}
        }
    }

    sender.shutdown().expect("shutdown");
    receiver.shutdown().expect("shutdown");
}

#[test]
fn blocking_channel_lifecycle() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .expect("runtime");
    let node = BlockingDeliveryNode::start(config(24030), runtime.handle().clone()).expect("node");

    let channel = node
        .create_channel(ChannelConfig {
            channel_id: "blocking-channel".into(),
            content_topic: TOPIC.into(),
            sender_id: "alice".into(),
        })
        .expect("create channel");
    assert!(channel.exists().expect("exists"));
    assert!(!channel.send(b"hello").expect("send").0.is_empty());
    assert!(!channel
        .send_ephemeral(b"hello")
        .expect("send ephemeral")
        .0
        .is_empty());
    channel.close().expect("close");
    assert!(!channel.exists().expect("exists after close"));

    node.shutdown().expect("shutdown");
}
