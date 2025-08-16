# OpenAce-Engine Makefile
# Cross-platform build automation

.PHONY: all build-all build-local clean install-targets help test

# Default target
all: build-local

# Build for all platforms
build-all:
	@echo "🚀 Building for all platforms..."
	@chmod +x scripts/build-all-platforms.sh
	@./scripts/build-all-platforms.sh

# Build for current platform only
build-local:
	@echo "🔨 Building for current platform..."
	cargo build --release

# Build for specific platform (usage: make build-target TARGET=x86_64-unknown-linux-gnu)
build-target:
	@if [ -z "$(TARGET)" ]; then \
		echo "❌ Please specify TARGET. Example: make build-target TARGET=x86_64-unknown-linux-gnu"; \
		exit 1; \
	fi
	@echo "🔨 Building for target: $(TARGET)"
	rustup target add $(TARGET) || true
	RUSTFLAGS="-C target-feature=+crt-static" cargo build --release --target=$(TARGET)

# Install all required targets
install-targets:
	@echo "📦 Installing all required targets..."
	rustup target add x86_64-pc-windows-gnu
	rustup target add aarch64-pc-windows-msvc
	rustup target add x86_64-apple-darwin
	rustup target add aarch64-apple-darwin
	rustup target add x86_64-unknown-linux-gnu
	rustup target add aarch64-unknown-linux-gnu
	@echo "✅ All targets installed"

# Clean build artifacts
clean:
	@echo "🧹 Cleaning build artifacts..."
	cargo clean
	rm -rf OpenAce-Builds
	@echo "✅ Clean completed"

# Run tests
test:
	@echo "🧪 Running tests..."
	cargo test

# Run clippy for code quality
clippy:
	@echo "📎 Running clippy..."
	cargo clippy -- -D warnings

# Format code
fmt:
	@echo "🎨 Formatting code..."
	cargo fmt

# Check code without building
check:
	@echo "🔍 Checking code..."
	cargo check

# Build documentation
docs:
	@echo "📚 Building documentation..."
	cargo doc --no-deps --open

# Quick development build (debug mode)
dev:
	@echo "⚡ Quick development build..."
	cargo build

# Run the binary
run:
	@echo "🏃 Running OpenAce-Engine..."
	cargo run --release

# Install dependencies and setup development environment
setup:
	@echo "🛠️  Setting up development environment..."
	rustup component add clippy rustfmt
	make install-targets
	@echo "✅ Development environment ready"

# Show available targets
show-targets:
	@echo "📋 Available build targets:"
	@echo "  • x86_64-pc-windows-gnu     (Windows x64)"
	@echo "  • aarch64-pc-windows-msvc   (Windows ARM64)"
	@echo "  • x86_64-apple-darwin       (macOS x64)"
	@echo "  • aarch64-apple-darwin      (macOS ARM64)"
	@echo "  • x86_64-unknown-linux-gnu  (Linux x64)"
	@echo "  • aarch64-unknown-linux-gnu (Linux ARM64)"

# Help
help:
	@echo "OpenAce-Engine Build System"
	@echo "==========================="
	@echo ""
	@echo "Available targets:"
	@echo "  all           - Build for current platform (default)"
	@echo "  build-all     - Build for all supported platforms"
	@echo "  build-local   - Build for current platform only"
	@echo "  build-target  - Build for specific target (requires TARGET=...)"
	@echo "  install-targets - Install all required Rust targets"
	@echo "  clean         - Clean all build artifacts"
	@echo "  test          - Run all tests"
	@echo "  clippy        - Run clippy for code quality"
	@echo "  fmt           - Format code with rustfmt"
	@echo "  check         - Check code without building"
	@echo "  docs          - Build and open documentation"
	@echo "  dev           - Quick development build (debug)"
	@echo "  run           - Run the binary"
	@echo "  setup         - Setup development environment"
	@echo "  show-targets  - Show available build targets"
	@echo "  help          - Show this help message"
	@echo ""
	@echo "Examples:"
	@echo "  make build-all                              # Build for all platforms"
	@echo "  make build-target TARGET=x86_64-apple-darwin  # Build for macOS x64"
	@echo "  make clean && make build-all                # Clean and rebuild all"