# logos-delivery

Async (tokio) Rust client for [Logos Delivery](https://github.com/logos-messaging/logos-delivery),
with reliable channels. Built on the bindings nim-ffi generates for `liblogosdelivery`.

```rust
let node = DeliveryNode::start(DeliveryConfig::default().tcp_port(60000)).await?;
node.wait_connected(Duration::from_secs(30)).await?;

node.subscribe("/my-app/1/chat/proto").await?;
let mut messages = Box::pin(node.messages());       // Stream<Item = ReceivedMessage>
let request_id = node.publish("/my-app/1/chat/proto", b"hi").await?;
let mut events = node.events();                     // broadcast::Receiver<DeliveryEvent>

let channel = node.create_channel(ChannelConfig { /* id, content_topic, sender_id */ }).await?;
channel.send(b"hello").await?;                      // ChannelEvent::{Received, Sent, Error, Lost}
```

## Linking

Set `LOGOS_DELIVERY_LIB_DIR` to the directory holding `liblogosdelivery`
(`make liblogosdelivery` in logos-delivery produces `build/`). Without it `cargo check`
passes but linking fails. `LOGOS_DELIVERY_RELOCATABLE=1` links the library in place for
bundling (iOS always does).

## Regenerating the FFI layer

`src/generated/` is nim-ffi's Rust output for the logos-delivery revision in
`../LOGOS_DELIVERY_REV`. After bumping logos-delivery (and so its pinned nim-ffi):

```
scripts/gen-bindings.sh /path/to/logos-delivery
```

## Limits

- Channels are created unencrypted; custom ciphers (`channel_create`'s function pointers) are not exposed yet.
- Two nodes in one process share SDS persistence, so a channel message between them is
  rejected as a replay; test channel delivery across processes.
- The node is stopped when the last `DeliveryNode` clone drops; call `shutdown()` to stop it earlier.
