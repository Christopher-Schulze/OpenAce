#!/bin/zsh

# Format code
cargo fmt

# Lint code
cargo clippy --all-targets -- -D warnings

# Update dependencies
cargo update

# Audit dependencies
cargo audit

# Clean build artifacts
cargo clean