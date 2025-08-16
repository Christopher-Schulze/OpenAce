# OpenAce Cross-Platform Build System

This directory contains scripts and tools for building OpenAce-Engine across multiple platforms and architectures.

## Quick Start

### Using Make (Recommended)

```bash
# Build for all platforms
make build-all

# Build for current platform only
make build-local

# Setup development environment
make setup

# Show all available commands
make help
```

### Using Scripts Directly

#### Linux/macOS
```bash
# Make script executable
chmod +x scripts/build-all-platforms.sh

# Run build script
./scripts/build-all-platforms.sh
```

#### Windows (PowerShell)
```powershell
# Build all platforms
.\scripts\build-all-platforms.ps1

# Build Windows only
.\scripts\build-all-platforms.ps1 -WindowsOnly

# Skip specific platforms
.\scripts\build-all-platforms.ps1 -SkipLinux -SkipMacOS
```

## Supported Platforms

The build system supports the following target platforms:

| Platform | Architecture | Target Triple | Binary Extension |
|----------|-------------|---------------|------------------|
| Windows | x64 (AMD64) | `x86_64-pc-windows-gnu` | `.exe` |
| Windows | ARM64 | `aarch64-pc-windows-msvc` | `.exe` |
| macOS | x64 (Intel) | `x86_64-apple-darwin` | none |
| macOS | ARM64 (Apple Silicon) | `aarch64-apple-darwin` | none |
| Linux | x64 (AMD64) | `x86_64-unknown-linux-gnu` | none |
| Linux | ARM64 | `aarch64-unknown-linux-gnu` | none |

## Output Structure

Builds are organized in the `OpenAce-Builds/` directory:

```
OpenAce-Builds/
├── windows_amd64/
│   └── OpenAce-Engine.exe
├── windows_arm64/
│   └── OpenAce-Engine.exe
├── macos_amd64/
│   └── OpenAce-Engine
├── macos_arm64/
│   └── OpenAce-Engine
├── linux_amd64/
│   └── OpenAce-Engine
└── linux_arm64/
    └── OpenAce-Engine
```

## Static Linking

All binaries are built with static linking enabled:

- **Windows**: Uses `crt-static` and static linking flags
- **Linux**: Uses `crt-static` and static linking for maximum portability
- **macOS**: Uses static linking where possible (some system libraries remain dynamic)

## Prerequisites

### Required Tools

1. **Rust Toolchain**
   ```bash
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   ```

2. **Cross-compilation Targets**
   ```bash
   make install-targets
   # OR manually:
   rustup target add x86_64-pc-windows-gnu
   rustup target add aarch64-pc-windows-msvc
   rustup target add x86_64-apple-darwin
   rustup target add aarch64-apple-darwin
   rustup target add x86_64-unknown-linux-gnu
   rustup target add aarch64-unknown-linux-gnu
   ```

### Platform-Specific Requirements

#### Linux (for Windows cross-compilation)
```bash
# Ubuntu/Debian
sudo apt-get install gcc-mingw-w64-x86-64

# For ARM64 Linux builds
sudo apt-get install gcc-aarch64-linux-gnu
```

#### macOS
```bash
# Xcode Command Line Tools
xcode-select --install
```

#### Windows
```powershell
# Visual Studio Build Tools or Visual Studio Community
# Download from: https://visualstudio.microsoft.com/downloads/
```

## Build Configuration

### Environment Variables

The build system uses the following environment variables:

- `RUSTFLAGS`: Set automatically for static linking
- `TARGET`: Used by Makefile for specific target builds

### Cargo Features

Available features (configured in `Cargo.toml`):

- `python`: Enable Python bindings
- `c-api`: Enable C API compatibility layer
- `default`: Standard feature set

### Build Profiles

- **Release Profile**: Optimized for production
  - LTO enabled
  - Debug symbols stripped
  - Panic = abort
  - Maximum optimization

- **Dev Profile**: Optimized for development
  - Fast compilation
  - Debug symbols included
  - No LTO

## Troubleshooting

### Common Issues

1. **Missing Target**
   ```
   Error: target 'x86_64-pc-windows-gnu' not found
   ```
   **Solution**: Run `make install-targets` or `rustup target add <target>`

2. **Linker Not Found**
   ```
   Error: linker 'x86_64-w64-mingw32-gcc' not found
   ```
   **Solution**: Install cross-compilation tools for your platform

3. **Build Fails on Windows**
   - Ensure Visual Studio Build Tools are installed
   - Try using PowerShell as Administrator
   - Check that Windows SDK is available

4. **Permission Denied (Linux/macOS)**
   ```bash
   chmod +x scripts/build-all-platforms.sh
   ```

### Debug Build Issues

1. **Enable Verbose Output**
   ```bash
   RUST_LOG=debug ./scripts/build-all-platforms.sh
   ```

2. **Check Individual Target**
   ```bash
   make build-target TARGET=x86_64-unknown-linux-gnu
   ```

3. **Verify Dependencies**
   ```bash
   cargo tree
   ```

## Performance Tips

1. **Parallel Builds**: The scripts build targets sequentially to avoid resource conflicts
2. **Disk Space**: Each target requires ~50-100MB, plan accordingly
3. **Build Time**: Full cross-platform build takes 5-15 minutes depending on hardware
4. **Incremental Builds**: Use `cargo build` for faster development iterations

## Contributing

When modifying the build system:

1. Test on multiple platforms
2. Update this documentation
3. Ensure backward compatibility
4. Add appropriate error handling
5. Update the Makefile if adding new targets

## License

Same as the main OpenAce project.