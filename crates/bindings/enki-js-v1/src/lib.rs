//! Browser-oriented JavaScript bindings for custom LLM-backed Enki agents.

#![allow(clippy::needless_pass_by_value)]

#[cfg(target_arch = "wasm32")]
mod wasm;

#[cfg(target_arch = "wasm32")]
pub use wasm::*;
