# Waku Rust bindings

[![Crates.io][crates-badge]][crates-url]
[![Documentation][docs-badge]][docs-url]
[![Build Status][actions-badge]][actions-url]
[![Codecov Status][codecov-badge]][codecov-url]

[crates-badge]: https://img.shields.io/crates/v/waku-bindings.svg
[crates-url]: https://crates.io/crates/waku-bindings
[docs-badge]: https://docs.rs/waku-bindings/badge.svg
[docs-url]: https://docs.rs/waku-bindings
[actions-badge]: https://github.com/logos-messaging/logos-messaging-rust-bindings/workflows/CI/badge.svg
[actions-url]: https://github.com/logos-messaging/logos-messaging-rust-bindings/actions/workflows/main.yml?query=workflow%3ACI+branch%3Amaster
[codecov-badge]: https://codecov.io/github/logos-messaging/logos-messaging-rust-bindings/branch/main/graph/badge.svg?token=H4CQWRUCUS
[codecov-url]: https://codecov.io/github/logos-messaging/logos-messaging-rust-bindings

Rust layer on top of [`logos-messaging-nim`](https://github.com/logos-messaging/logos-messaging-nim) [C FFI bindings](https://github.com/logos-messaging/logos-messaging-nim/blob/master/library/libwaku.h).

# How to build and run the toy-chat example

- After cloning the repo run `git submodule update --init --recursive` to init all submodules.
- Run the command `cargo build` to build the bindings
- To run the toy-chat example

  ```
  cd examples/toy-chat
  cargo run <name>
  ```


## About Waku

[Waku](https://waku.org/) is a family of robust and censorship-resistant communication protocols enabling privacy-focused messaging for Web3 applications.

Private. Secure. Runs anywhere.

Read the [Waku docs](https://docs.waku.org/)
