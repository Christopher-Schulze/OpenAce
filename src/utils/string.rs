//! Compatibility shim for OpenAce Rust
//! This module forwards `utils::string` to the implementation in `utils::strings`.
//! It preserves legacy import paths while keeping a single source of truth.

// Re-export everything from the sibling module declared in `utils/mod.rs`
pub use super::strings::*;
