use logos_delivery::{DeliveryConfig, DeliveryError, DeliveryNode};
use serial_test::serial;

fn config(port: u16) -> DeliveryConfig {
    DeliveryConfig::default()
        .no_preset()
        .tcp_port(port)
        .discv5_udp_port(port + 1000)
        .option("clusterId", 99)
        .option("numShardsInNetwork", 8)
        .option("shards", (0..8).collect::<Vec<_>>())
        .option("localStoragePath", "./data-logos-delivery-version-test")
}

#[tokio::test]
#[serial]
async fn starts_against_the_linked_library() {
    let node = DeliveryNode::start(config(62010)).await.expect("start");
    node.shutdown().await.expect("shutdown");
}

#[tokio::test]
async fn fails_before_creating_a_node_when_library_is_too_old() {
    let err = DeliveryNode::start(config(62020).require_min_library_version("999.0.0"))
        .await
        .err()
        .expect("must fail");
    assert!(
        matches!(err, DeliveryError::VersionMismatch { ref expected, .. } if expected == ">= 999.0.0"),
        "unexpected error: {err}"
    );
}

#[tokio::test]
async fn skip_version_check_bypasses_the_check() {
    let cfg = config(62030)
        .require_min_library_version("999.0.0")
        .skip_version_check();
    let node = DeliveryNode::start(cfg).await.expect("start");
    node.shutdown().await.expect("shutdown");
}
