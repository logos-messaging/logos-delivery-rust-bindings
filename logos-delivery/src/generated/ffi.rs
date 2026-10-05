use std::os::raw::{c_char, c_int, c_void};

pub type FFICallback = unsafe extern "C" fn(
    ret: c_int,
    msg: *const c_char,
    len: usize,
    user_data: *mut c_void,
);

#[link(name = "logosdelivery")]
extern "C" {
    pub fn logosdelivery_create_node(req_cbor: *const u8, req_cbor_len: usize, callback: FFICallback, user_data: *mut c_void) -> *mut c_void;
    pub fn logosdelivery_start_node(ctx: *mut c_void, callback: FFICallback, user_data: *mut c_void, req_cbor: *const u8, req_cbor_len: usize) -> c_int;
    pub fn logosdelivery_stop_node(ctx: *mut c_void, callback: FFICallback, user_data: *mut c_void, req_cbor: *const u8, req_cbor_len: usize) -> c_int;
    /// Returns the node's current connection status: `Disconnected`,
    /// `PartiallyConnected` or `Connected`. `onConnectionStatusChange` reports
    /// only transitions, so this is how a late listener reads the current one.
    pub fn logosdelivery_get_connection_status(ctx: *mut c_void, callback: FFICallback, user_data: *mut c_void, req_cbor: *const u8, req_cbor_len: usize) -> c_int;
    /// Safety net for a host that skips `stop_node` (#4108): nim-ffi recycles the
    /// worker rather than joining it, so an unstopped node keeps running.
    /// The forwarders registered at create live until here, with the node's
    /// broker scope; `teardownFFIEventScope` is the other end of create.
    pub fn logosdelivery_destroy(ctx: *mut c_void) -> c_int;
    pub fn logosdelivery_subscribe(ctx: *mut c_void, callback: FFICallback, user_data: *mut c_void, req_cbor: *const u8, req_cbor_len: usize) -> c_int;
    pub fn logosdelivery_unsubscribe(ctx: *mut c_void, callback: FFICallback, user_data: *mut c_void, req_cbor: *const u8, req_cbor_len: usize) -> c_int;
    pub fn logosdelivery_send(ctx: *mut c_void, callback: FFICallback, user_data: *mut c_void, req_cbor: *const u8, req_cbor_len: usize) -> c_int;
    /// Returns, as a JSON array of strings, all available node info item ids that
    /// can be queried with `get_node_info`.
    pub fn logosdelivery_get_available_node_info_ids(ctx: *mut c_void, callback: FFICallback, user_data: *mut c_void, req_cbor: *const u8, req_cbor_len: usize) -> c_int;
    /// Returns the content of the node info item with the given id if it exists.
    /// The content is a plain string, not JSON: a peer id, an ENR URI, a
    /// comma-separated multiaddress list or the Prometheus metrics text.
    pub fn logosdelivery_get_node_info(ctx: *mut c_void, callback: FFICallback, user_data: *mut c_void, req_cbor: *const u8, req_cbor_len: usize) -> c_int;
    /// Returns information about the accepted config items.
    pub fn logosdelivery_get_available_configs(ctx: *mut c_void, callback: FFICallback, user_data: *mut c_void, req_cbor: *const u8, req_cbor_len: usize) -> c_int;
    /// Installs (or replaces) the service-discovery plugin.
    /// pluginPtr - address of an `LdServiceDiscoveryPlugin`, borrowed for the call
    pub fn logosdelivery_set_service_discovery_plugin(ctx: *mut c_void, callback: FFICallback, user_data: *mut c_void, req_cbor: *const u8, req_cbor_len: usize) -> c_int;
    /// What the host must set up before `start`: whether service discovery is
    /// expected from a plugin, and the DHT bootstrap peers the node's
    /// configuration resolves to, presets included. Reply JSON:
    /// {"externalServiceDiscovery": bool, "bootstrapNodes": ["/dns4/.../p2p/16Uiu..."]}
    pub fn logosdelivery_get_discovery_requirements(ctx: *mut c_void, callback: FFICallback, user_data: *mut c_void, req_cbor: *const u8, req_cbor_len: usize) -> c_int;
    /// Removes the installed plugin; discovery verbs fail until a new one arrives.
    pub fn logosdelivery_clear_service_discovery_plugin(ctx: *mut c_void, callback: FFICallback, user_data: *mut c_void, req_cbor: *const u8, req_cbor_len: usize) -> c_int;
    /// returns a comma-separated string of peerIDs
    pub fn waku_get_peerids_from_peerstore(ctx: *mut c_void, callback: FFICallback, user_data: *mut c_void, req_cbor: *const u8, req_cbor_len: usize) -> c_int;
    pub fn waku_connect(ctx: *mut c_void, callback: FFICallback, user_data: *mut c_void, req_cbor: *const u8, req_cbor_len: usize) -> c_int;
    pub fn waku_disconnect_peer_by_id(ctx: *mut c_void, callback: FFICallback, user_data: *mut c_void, req_cbor: *const u8, req_cbor_len: usize) -> c_int;
    pub fn waku_disconnect_all_peers(ctx: *mut c_void, callback: FFICallback, user_data: *mut c_void, req_cbor: *const u8, req_cbor_len: usize) -> c_int;
    pub fn waku_dial_peer(ctx: *mut c_void, callback: FFICallback, user_data: *mut c_void, req_cbor: *const u8, req_cbor_len: usize) -> c_int;
    pub fn waku_dial_peer_by_id(ctx: *mut c_void, callback: FFICallback, user_data: *mut c_void, req_cbor: *const u8, req_cbor_len: usize) -> c_int;
    /// returns a JSON string mapping peerIDs to objects with protocols and addresses
    pub fn waku_get_connected_peers_info(ctx: *mut c_void, callback: FFICallback, user_data: *mut c_void, req_cbor: *const u8, req_cbor_len: usize) -> c_int;
    /// returns a comma-separated string of peerIDs
    pub fn waku_get_connected_peers(ctx: *mut c_void, callback: FFICallback, user_data: *mut c_void, req_cbor: *const u8, req_cbor_len: usize) -> c_int;
    /// returns a comma-separated string of peerIDs that mount the given protocol
    pub fn waku_get_peerids_by_protocol(ctx: *mut c_void, callback: FFICallback, user_data: *mut c_void, req_cbor: *const u8, req_cbor_len: usize) -> c_int;
    /// Updates the bootnode list used for discovering new peers via DiscoveryV5
    /// bootnodes - JSON array containing the bootnode ENRs i.e. `["enr:...", "enr:..."]`
    pub fn waku_discv5_update_bootnodes(ctx: *mut c_void, callback: FFICallback, user_data: *mut c_void, req_cbor: *const u8, req_cbor_len: usize) -> c_int;
    /// returns a comma-separated string of bootstrap nodes' multiaddresses
    pub fn waku_dns_discovery(ctx: *mut c_void, callback: FFICallback, user_data: *mut c_void, req_cbor: *const u8, req_cbor_len: usize) -> c_int;
    pub fn waku_start_discv5(ctx: *mut c_void, callback: FFICallback, user_data: *mut c_void, req_cbor: *const u8, req_cbor_len: usize) -> c_int;
    pub fn waku_stop_discv5(ctx: *mut c_void, callback: FFICallback, user_data: *mut c_void, req_cbor: *const u8, req_cbor_len: usize) -> c_int;
    pub fn waku_peer_exchange_request(ctx: *mut c_void, callback: FFICallback, user_data: *mut c_void, req_cbor: *const u8, req_cbor_len: usize) -> c_int;
    pub fn waku_version(ctx: *mut c_void, callback: FFICallback, user_data: *mut c_void, req_cbor: *const u8, req_cbor_len: usize) -> c_int;
    /// returns a comma-separated string of the listen addresses
    pub fn waku_listen_addresses(ctx: *mut c_void, callback: FFICallback, user_data: *mut c_void, req_cbor: *const u8, req_cbor_len: usize) -> c_int;
    pub fn waku_get_my_enr(ctx: *mut c_void, callback: FFICallback, user_data: *mut c_void, req_cbor: *const u8, req_cbor_len: usize) -> c_int;
    pub fn waku_get_my_peerid(ctx: *mut c_void, callback: FFICallback, user_data: *mut c_void, req_cbor: *const u8, req_cbor_len: usize) -> c_int;
    pub fn waku_get_metrics(ctx: *mut c_void, callback: FFICallback, user_data: *mut c_void, req_cbor: *const u8, req_cbor_len: usize) -> c_int;
    pub fn waku_is_online(ctx: *mut c_void, callback: FFICallback, user_data: *mut c_void, req_cbor: *const u8, req_cbor_len: usize) -> c_int;
    pub fn waku_ping_peer(ctx: *mut c_void, callback: FFICallback, user_data: *mut c_void, req_cbor: *const u8, req_cbor_len: usize) -> c_int;
    /// returns a comma-separated string of peerIDs
    pub fn waku_relay_get_peers_in_mesh(ctx: *mut c_void, callback: FFICallback, user_data: *mut c_void, req_cbor: *const u8, req_cbor_len: usize) -> c_int;
    pub fn waku_relay_get_num_peers_in_mesh(ctx: *mut c_void, callback: FFICallback, user_data: *mut c_void, req_cbor: *const u8, req_cbor_len: usize) -> c_int;
    /// Returns the list of all connected peers to an specific pubsub topic
    pub fn waku_relay_get_connected_peers(ctx: *mut c_void, callback: FFICallback, user_data: *mut c_void, req_cbor: *const u8, req_cbor_len: usize) -> c_int;
    pub fn waku_relay_get_num_connected_peers(ctx: *mut c_void, callback: FFICallback, user_data: *mut c_void, req_cbor: *const u8, req_cbor_len: usize) -> c_int;
    /// Protects a shard with a public key
    pub fn waku_relay_add_protected_shard(ctx: *mut c_void, callback: FFICallback, user_data: *mut c_void, req_cbor: *const u8, req_cbor_len: usize) -> c_int;
    pub fn waku_relay_subscribe(ctx: *mut c_void, callback: FFICallback, user_data: *mut c_void, req_cbor: *const u8, req_cbor_len: usize) -> c_int;
    pub fn waku_relay_unsubscribe(ctx: *mut c_void, callback: FFICallback, user_data: *mut c_void, req_cbor: *const u8, req_cbor_len: usize) -> c_int;
    pub fn waku_relay_publish(ctx: *mut c_void, callback: FFICallback, user_data: *mut c_void, req_cbor: *const u8, req_cbor_len: usize) -> c_int;
    pub fn waku_default_pubsub_topic(ctx: *mut c_void, callback: FFICallback, user_data: *mut c_void, req_cbor: *const u8, req_cbor_len: usize) -> c_int;
    pub fn waku_content_topic(ctx: *mut c_void, callback: FFICallback, user_data: *mut c_void, req_cbor: *const u8, req_cbor_len: usize) -> c_int;
    pub fn waku_pubsub_topic(ctx: *mut c_void, callback: FFICallback, user_data: *mut c_void, req_cbor: *const u8, req_cbor_len: usize) -> c_int;
    pub fn waku_store_query(ctx: *mut c_void, callback: FFICallback, user_data: *mut c_void, req_cbor: *const u8, req_cbor_len: usize) -> c_int;
    pub fn waku_lightpush_publish(ctx: *mut c_void, callback: FFICallback, user_data: *mut c_void, req_cbor: *const u8, req_cbor_len: usize) -> c_int;
    pub fn waku_filter_subscribe(ctx: *mut c_void, callback: FFICallback, user_data: *mut c_void, req_cbor: *const u8, req_cbor_len: usize) -> c_int;
    pub fn waku_filter_unsubscribe(ctx: *mut c_void, callback: FFICallback, user_data: *mut c_void, req_cbor: *const u8, req_cbor_len: usize) -> c_int;
    pub fn waku_filter_unsubscribe_all(ctx: *mut c_void, callback: FFICallback, user_data: *mut c_void, req_cbor: *const u8, req_cbor_len: usize) -> c_int;
    /// `encryptFn`/`decryptFn` are `LogosDeliveryCryptoFn` pointers cast to
    /// `uint64`, and all three zero means an unencrypted channel. The cipher
    /// is fixed for the channel's life.
    ///
    /// `userData` is what lets one C function serve several channels: a
    /// function pointer carries no state, so the same `my_encrypt` used on two
    /// channels is the same address both times and cannot tell them apart.
    /// Whatever is passed here comes back as the callback's first argument on
    /// every call, so it can point at this channel's key.
    pub fn logosdelivery_channel_create(ctx: *mut c_void, callback: FFICallback, user_data: *mut c_void, req_cbor: *const u8, req_cbor_len: usize) -> c_int;
    /// Returns `"true"` or `"false"`; a missing channel is not an error.
    pub fn logosdelivery_channel_exists(ctx: *mut c_void, callback: FFICallback, user_data: *mut c_void, req_cbor: *const u8, req_cbor_len: usize) -> c_int;
    /// `messageJson` carries `{ "payload": <base64>, "ephemeral": <bool> }`.
    pub fn logosdelivery_channel_send(ctx: *mut c_void, callback: FFICallback, user_data: *mut c_void, req_cbor: *const u8, req_cbor_len: usize) -> c_int;
    pub fn logosdelivery_channel_close(ctx: *mut c_void, callback: FFICallback, user_data: *mut c_void, req_cbor: *const u8, req_cbor_len: usize) -> c_int;
    pub fn logosdelivery_add_event_listener(ctx: *mut c_void, event_name: *const c_char, callback: FFICallback, user_data: *mut c_void) -> u64;
    pub fn logosdelivery_remove_event_listener(ctx: *mut c_void, listener_id: u64) -> c_int;
    /// Stop every context the library still holds and join their threads.
    /// Call it before the process exits when a context is still alive, or when a
    /// static proc built the shared context.
    /// Returns 0 when every context stopped, 1 when one was left running.
    pub fn logosdelivery_shutdown() -> c_int;
}
