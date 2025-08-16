#!/bin/zsh

# Monitor memory usage
cargo flamegraph --bin openace -- --some-args

# Performance profile
cargo prof --bin openace