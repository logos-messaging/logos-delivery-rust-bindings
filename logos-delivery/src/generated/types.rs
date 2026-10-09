use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogosdeliveryCreateNodeCtorReq {
    #[serde(rename = "configJson")]
    pub config_json: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogosdeliveryStartNodeReq {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogosdeliveryStopNodeReq {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogosdeliveryGetConnectionStatusReq {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogosdeliverySubscribeReq {
    #[serde(rename = "contentTopicStr")]
    pub content_topic_str: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogosdeliveryUnsubscribeReq {
    #[serde(rename = "contentTopicStr")]
    pub content_topic_str: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogosdeliverySendReq {
    #[serde(rename = "messageJson")]
    pub message_json: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogosdeliveryGetAvailableNodeInfoIdsReq {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogosdeliveryGetNodeInfoReq {
    #[serde(rename = "nodeInfoId")]
    pub node_info_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogosdeliveryGetAvailableConfigsReq {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogosdeliverySetServiceDiscoveryPluginReq {
    #[serde(rename = "pluginPtr")]
    pub plugin_ptr: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogosdeliveryGetDiscoveryRequirementsReq {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogosdeliveryClearServiceDiscoveryPluginReq {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WakuGetPeeridsFromPeerstoreReq {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WakuConnectReq {
    #[serde(rename = "peerMultiAddr")]
    pub peer_multi_addr: String,
    #[serde(rename = "timeoutMs")]
    pub timeout_ms: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WakuDisconnectPeerByIdReq {
    #[serde(rename = "peerId")]
    pub peer_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WakuDisconnectAllPeersReq {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WakuDialPeerReq {
    #[serde(rename = "peerMultiAddr")]
    pub peer_multi_addr: String,
    pub protocol: String,
    #[serde(rename = "timeoutMs")]
    pub timeout_ms: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WakuDialPeerByIdReq {
    #[serde(rename = "peerId")]
    pub peer_id: String,
    pub protocol: String,
    #[serde(rename = "timeoutMs")]
    pub timeout_ms: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WakuGetConnectedPeersInfoReq {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WakuGetConnectedPeersReq {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WakuGetPeeridsByProtocolReq {
    pub protocol: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WakuDiscv5UpdateBootnodesReq {
    pub bootnodes: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WakuDnsDiscoveryReq {
    #[serde(rename = "enrTreeUrl")]
    pub enr_tree_url: String,
    #[serde(rename = "nameDnsServer")]
    pub name_dns_server: String,
    #[serde(rename = "timeoutMs")]
    pub timeout_ms: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WakuStartDiscv5Req {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WakuStopDiscv5Req {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WakuPeerExchangeRequestReq {
    #[serde(rename = "numPeers")]
    pub num_peers: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WakuVersionReq {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WakuListenAddressesReq {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WakuGetMyEnrReq {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WakuGetMyPeeridReq {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WakuGetMetricsReq {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WakuIsOnlineReq {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WakuPingPeerReq {
    #[serde(rename = "peerAddr")]
    pub peer_addr: String,
    #[serde(rename = "timeoutMs")]
    pub timeout_ms: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WakuRelayGetPeersInMeshReq {
    #[serde(rename = "pubSubTopic")]
    pub pub_sub_topic: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WakuRelayGetNumPeersInMeshReq {
    #[serde(rename = "pubSubTopic")]
    pub pub_sub_topic: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WakuRelayGetConnectedPeersReq {
    #[serde(rename = "pubSubTopic")]
    pub pub_sub_topic: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WakuRelayGetNumConnectedPeersReq {
    #[serde(rename = "pubSubTopic")]
    pub pub_sub_topic: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WakuRelayAddProtectedShardReq {
    #[serde(rename = "clusterId")]
    pub cluster_id: u16,
    #[serde(rename = "shardId")]
    pub shard_id: u16,
    #[serde(rename = "publicKey")]
    pub public_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WakuRelaySubscribeReq {
    #[serde(rename = "pubSubTopic")]
    pub pub_sub_topic: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WakuRelayUnsubscribeReq {
    #[serde(rename = "pubSubTopic")]
    pub pub_sub_topic: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WakuRelayPublishReq {
    #[serde(rename = "pubSubTopic")]
    pub pub_sub_topic: String,
    #[serde(rename = "jsonWakuMessage")]
    pub json_waku_message: String,
    #[serde(rename = "timeoutMs")]
    pub timeout_ms: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WakuDefaultPubsubTopicReq {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WakuContentTopicReq {
    #[serde(rename = "appName")]
    pub app_name: String,
    #[serde(rename = "appVersion")]
    pub app_version: u32,
    #[serde(rename = "contentTopicName")]
    pub content_topic_name: String,
    pub encoding: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WakuPubsubTopicReq {
    #[serde(rename = "topicName")]
    pub topic_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WakuStoreQueryReq {
    #[serde(rename = "jsonQuery")]
    pub json_query: String,
    #[serde(rename = "peerAddr")]
    pub peer_addr: String,
    #[serde(rename = "timeoutMs")]
    pub timeout_ms: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WakuLightpushPublishReq {
    #[serde(rename = "pubSubTopic")]
    pub pub_sub_topic: String,
    #[serde(rename = "jsonWakuMessage")]
    pub json_waku_message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WakuFilterSubscribeReq {
    #[serde(rename = "pubSubTopic")]
    pub pub_sub_topic: String,
    #[serde(rename = "contentTopics")]
    pub content_topics: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WakuFilterUnsubscribeReq {
    #[serde(rename = "pubSubTopic")]
    pub pub_sub_topic: String,
    #[serde(rename = "contentTopics")]
    pub content_topics: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WakuFilterUnsubscribeAllReq {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogosdeliveryChannelCreateReq {
    #[serde(rename = "channelIdStr")]
    pub channel_id_str: String,
    #[serde(rename = "contentTopicStr")]
    pub content_topic_str: String,
    #[serde(rename = "senderIdStr")]
    pub sender_id_str: String,
    #[serde(rename = "encryptFn")]
    pub encrypt_fn: u64,
    #[serde(rename = "decryptFn")]
    pub decrypt_fn: u64,
    #[serde(rename = "userData")]
    pub user_data: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogosdeliveryChannelExistsReq {
    #[serde(rename = "channelIdStr")]
    pub channel_id_str: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogosdeliveryChannelSendReq {
    #[serde(rename = "channelIdStr")]
    pub channel_id_str: String,
    #[serde(rename = "messageJson")]
    pub message_json: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogosdeliveryChannelCloseReq {
    #[serde(rename = "channelIdStr")]
    pub channel_id_str: String,
}
