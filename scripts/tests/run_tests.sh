#!/bin/zsh

# Run all unit tests
cargo test --lib

# Run integration tests
cargo test --test integration

# Run performance tests
cargo bench

# Generate coverage
cargo tarpaulin --out Html