# logos-delivery Rust Bindings

[Crates.io][crates-url]
[Documentation][docs-url]
[Build Status][actions-url]
[Codecov Status][codecov-url]

Rust bindings for [logos-delivery](https://github.com/logos-messaging/logos-delivery) (Waku) v0.38.0-beta,
built on top of the [C FFI](https://github.com/logos-messaging/logos-delivery/blob/master/library/libwaku.h).

## Prerequisites

- **Rust** (stable) — [rustup.rs](https://rustup.rs)
- **Nim 2.x** — required to compile the native `libwaku` library
- 
- **Make**
- **GCC or Clang**

## Setup

Clone the repository:

```bash
git clone https://github.com/logos-messaging/logos-delivery-rust-bindings.git
cd logos-delivery-rust-bindings
```

The first `cargo build` / `cargo run` automatically:

1. Initializes and updates git submodules
2. Compiles the native `libwaku` static library via `make libwaku STATIC=1`
3. Generates Rust FFI bindings via `bindgen`

## Running the Examples

### basic

Two Waku nodes in the same process. Node 1 publishes a message; node 2 receives it via relay subscription.

```bash
cd examples/basic
cargo run
```

### toy-chat

Multi-participant chat room over Waku relay. Pass your nickname as argument.

```bash
cd examples/toy-chat
cargo run "Alice"
```

Start another instance in a separate terminal (or on another machine) to chat:

```bash
cargo run "Bob"
```

### tic-tac-toe-gui

Multiplayer tic-tac-toe with a native GUI (eframe). Start two instances — locally or on separate machines.

```bash
cd examples/tic-tac-toe-gui
cargo run
```

## Running the Tests

Tests start real Waku nodes and bind to local TCP ports, so they **must run serially**:

```bash
# from repo root
cargo test -p waku-bindings -- --test-threads=1

# or from waku-bindings/
cd waku-bindings
cargo test
```

To see log output:

```bash
cargo test -- --nocapture
```

## Crates

| Crate                            | Description                    |
| -------------------------------- | ------------------------------ |
| [`waku-bindings`](waku-bindings/) | High-level Rust API            |
| [`waku-sys`](waku-sys/)           | Low-level bindgen FFI bindings |

## About Waku

[Waku](https://waku.org/) is a family of robust, censorship-resistant communication protocols for Web3. Private. Secure. Runs anywhere.

Read the [Waku docs](https://docs.waku.org/)

[crates-badge]: https://img.shields.io/crates/v/waku-bindings.svg
[crates-url]: https://crates.io/crates/waku-bindings
[docs-badge]: https://docs.rs/waku-bindings/badge.svg
[docs-url]: https://docs.rs/waku-bindings
[actions-badge]: https://github.com/logos-messaging/logos-delivery-rust-bindings/workflows/CI/badge.svg
[actions-url]: https://github.com/logos-messaging/logos-delivery-rust-bindings/actions/workflows/main.yml?query=workflow%3ACI+branch%3Amaster
[codecov-badge]: https://codecov.io/github/logos-messaging/logos-delivery-rust-bindings/branch/main/graph/badge.svg?token=H4CQWRUCUS
[codecov-url]: https://codecov.io/github/logos-messaging/logos-delivery-rust-bindings
