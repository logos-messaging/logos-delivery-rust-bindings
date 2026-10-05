//! Output of nim-ffi's Rust generator for liblogosdelivery. Do not edit by
//! hand: run `scripts/gen-bindings.sh` against the logos-delivery revision in
//! `LOGOS_DELIVERY_REV` instead.
//!
//! `api` resolves its siblings through `super::`, so the three stay sibling modules.
#![allow(clippy::all, dead_code)]

pub(crate) mod ffi;
pub(crate) mod types;
pub(crate) mod api;
