#!/bin/zsh

# Generate documentation
cargo doc --open

# Run doc tests
cargo test --doc