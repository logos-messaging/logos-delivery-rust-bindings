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

## Blocking API

Synchronous callers (no tokio) use `logos_delivery::blocking::BlockingDeliveryNode`: it owns a
two-worker runtime and mirrors the node (`start`, `subscribe`, `publish`, `wait_connected`,
`create_channel`, `shutdown`, ...). `inbound_queue(|msg| Option<T>)` and `events()` give
bounded `crossbeam_channel::Receiver`s fed off the FFI thread. Its methods, and dropping the
last clone, panic inside a tokio runtime (like `Runtime::block_on`): use `DeliveryNode` there.

## Linking

Set `LOGOS_DELIVERY_LIB_DIR` to the directory holding `liblogosdelivery`
(`make liblogosdelivery` in logos-delivery produces `build/`). Without it `cargo check`
passes but linking fails. `LOGOS_DELIVERY_RELOCATABLE=1` links the library in place for
bundling (iOS always does).

## Library version check

`DeliveryNode::start` first checks (once per process) that the linked library exports
`logosdelivery_version` and reports a nimble version at least `MIN_LIBRARY_VERSION`
(recorded by the generator), else it returns `DeliveryError::VersionMismatch`. The git
hash is not compared, so newer compatible libraries pass; an unparsable version only
logs a warning. Custom builds can opt out with `DeliveryConfig::skip_version_check()`.
The nimble version is coarse: it catches pre-CBOR-FFI libraries, not every breaking change
within one version.

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
