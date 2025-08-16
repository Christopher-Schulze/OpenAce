# OpenAce Engine

<p align="center">
  <img src="image.png" alt="OpenAce Engine Logo" width="200"/>
</p>

<p align="center">
  <a href="https://www.rust-lang.org/"><img src="https://img.shields.io/badge/Rust-000000?style=for-the-badge&logo=rust&logoColor=white" alt="Rust"></a>
  <a href="#"><img src="https://img.shields.io/badge/P2P-Streaming-blue?style=for-the-badge" alt="P2P Streaming"></a>
  <a href="./docs/LICENSE"><img src="https://img.shields.io/badge/License-MIT-green?style=for-the-badge" alt="License"></a>
  <a href="https://github.com/Christopher-Schulze/OpenAce"><img src="https://img.shields.io/github/last-commit/Christopher-Schulze/OpenAce?style=for-the-badge" alt="Last Commit"></a>
</p>

## Overview

OpenAce is an open, high‑performance P2P streaming platform built in Rust. Our primary goal is to define an open streaming protocol that is fully compatible with the Ace Stream ecosystem, so existing tools and content continue to work within a free and transparent environment. In parallel, we deliver a modern, state‑of‑the‑art P2P stack focused on performance and reliability, with planned hardware acceleration for current streaming devices (e.g., Android TV SoCs) and broad, first‑class support across platforms where Ace Stream is limited.

For details see:
- [docs/DOCUMENTATION.md](./docs/DOCUMENTATION.md)
- [docs/architecture.md](./docs/architecture.md)

## Highlights

- **Async core**: Tokio-based async I/O, zero-cost abstractions, lock-free structures where it matters
- **Predictable memory**: Custom memory pools and zero-copy buffers for consistent latency
- **Self-observability**: Real-time metrics, structured tracing, and health scoring across engines
- **Network savvy**: Connection pooling, adaptive rate control, and efficient message handling
- **Media pipeline**: Adaptive bitrate streaming with smart caching

## Features

- **Cross-Platform Support**: Static binaries for Windows, macOS, Linux, and Android
- **P2P Streaming**: Efficient peer-to-peer content delivery
- **Planned Hardware Acceleration**: Optimization for various SoCs in development.
- **Modular Architecture**: Easy extension and maintenance with trait-based design
- **Memory Safety**: Leveraging Rust's ownership system
- **Comprehensive Logging**: Centralized logging infrastructure with health monitoring
- **Real-time Streaming**: Low-latency live stream processing with optional chat features
- **Adaptive Bitrate**: Multiple quality levels with automatic switching
- **Engine Management**: Centralized orchestration of all engine instances

Status: see "Production Status" in [docs/DOCUMENTATION.md](./docs/DOCUMENTATION.md) (currently WIP / not production-ready).

## Quickstart

### Build and Run in 60 Seconds
1. Install Rust: `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`
2. Clone repository: `git clone https://github.com/Christopher-Schulze/OpenAce.git`
3. Build: `cd OpenAce && cargo build --release`
4. Run: `./target/release/OpenAce-Engine --config config.toml`
5. Play stream: Use a compatible player with transport URL.

Minimal config.toml:
```toml
[engine]
log_level = "info"
bind_address = "0.0.0.0:8080"
```

## Installation

### Prerequisites
- Rust 1.70.0 or later
- Platform-specific build tools (see documentation)

### Building from Source

```bash
git clone https://github.com/Christopher-Schulze/OpenAce.git
cd OpenAce
cargo build --release
```

Pre-built binaries are published under `releases/` in this repository. For cross-platform builds, use `scripts/run_builds.sh` to produce timestamped release artifacts for macOS (x64/arm64) and other targets.

## Usage

Run the engine:

```bash
./target/release/OpenAce-Engine
```

For development:

```bash
cargo run
```

OpenAce supports the following command-line flags:
- `--config <PATH>`: Path to configuration file (default: config.toml)
- `--log-level <LEVEL>`: Set log level (trace, debug, info, warn, error)
- `--help`: Show help message

Example: `OpenAce-Engine --config myconfig.toml --log-level debug`

Configuration Example:

```toml
[engine]
log_level = "info"
bind_address = "0.0.0.0:8080"
max_peers = 100

[streaming]
buffer_size = 4096
# hardware_acceleration = true (coming soon)

[security]
tls_enabled = true
```

See [DOCUMENTATION.md](./docs/DOCUMENTATION.md) for advanced usage and configuration.

## Architecture Overview

OpenAce features a modular, trait-based architecture with memory safety and thread safety at its core. Key components include:
- **Main Engine**: Centralized coordination
- **Live Engine**: Real-time streaming
- **Transport Engine**: P2P networking
- **Streamer Engine**: HTTP streaming with hardware acceleration
- **Segmenter Engine**: Media segmentation

For more details, refer to the full architecture in [docs/architecture.md](./docs/architecture.md) and the technical reference in [docs/DOCUMENTATION.md](./docs/DOCUMENTATION.md).

## Compatibility

OpenAce aims for high interoperability with the existing Ace Stream ecosystem. Current support includes partial compatibility for Ace Stream streams and peer connections. 

## Contributing

Contributions welcome! Please follow:
1. Fork the repository
2. Create feature branch
3. Add tests and documentation
4. Submit pull request

See contributing guidelines and scripts in `docs/` and `scripts/`. Useful scripts:

```bash
bash scripts/run_tests.sh           # run tests
bash scripts/run_builds.sh          # cross-platform builds
bash scripts/run_maintenance.sh     # fmt, clippy, audit, cleanup
bash scripts/run_documentation.sh   # docs build
```

## License

This project is licensed under the MIT License - see the [LICENSE](./docs/LICENSE) file for details.