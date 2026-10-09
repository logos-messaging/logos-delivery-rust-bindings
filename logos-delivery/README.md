# logos-delivery

Async (tokio) Rust client for [Logos Delivery](https://github.com/logos-messaging/logos-delivery),
with reliable channels. Built on the bindings nim-ffi generates for `liblogosdelivery`.

```rust
let node = DeliveryNode::start(DeliveryConfig::default().tcp_port(60000)).await?;
node.wait_connected(Duration::from_secs(30)).await?; // or DeliveryConfig::wait_for_connection(..) to wait inside start

node.subscribe("/my-app/1/chat/proto").await?;
let mut messages = Box::pin(node.messages());       // Stream<Item = ReceivedMessage>
let request_id = node.publish("/my-app/1/chat/proto", b"hi").await?;
let mut events = node.events();                     // broadcast::Receiver<DeliveryEvent>

let channel = node.create_channel(ChannelConfig { /* id, content_topic, sender_id */ }).await?;
channel.send(b"hello").await?;                      // ChannelEvent::{Received, Sent, Error, Lost}
```

## Blocking API

Synchronous callers (no tokio) use `logos_delivery::blocking::BlockingDeliveryNode`: it is driven
on a tokio `Handle` the caller passes to `start(config, handle)`; the crate never creates a
runtime. That runtime needs the time driver (e.g. `enable_all()`) and must be multi-threaded or
driven elsewhere. It mirrors the node (`subscribe`, `publish`, `wait_connected`,
`create_channel`, `shutdown`, ...). `inbound_queue(|msg| Option<T>)` and `events()` give
bounded `crossbeam_channel::Receiver`s fed off the FFI thread. Its methods, and dropping the
last clone, panic inside a tokio runtime (like `Handle::block_on`): use `DeliveryNode` there.

## Linking

Set `LOGOS_DELIVERY_LIB_DIR` to the directory holding `liblogosdelivery` (an absolute path; a relative one is resolved against the directory cargo is run from)
(`make liblogosdelivery` in logos-delivery produces `build/`). Without it `cargo check`
passes but linking fails. `LOGOS_DELIVERY_RELOCATABLE=1` links the library in place for
bundling (iOS and Android always do). The default build stamps an absolute install name on
a copy of the library (needs `patchelf` on Linux); if that fails the build stops, and
`LOGOS_DELIVERY_ALLOW_UNSTAMPED=1` links it in place for this crate's own tests only.
Windows is not supported. Requires Rust 1.77.

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
