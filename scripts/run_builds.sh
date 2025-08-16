#!/bin/bash
# OpenAce Cross-Platform Build Script (Bash)
# Builds static binaries for all supported platforms and architectures

set -e

# Build configuration
PROJECT_NAME="OpenAce-Engine"
BUILD_DIR="releases"
CARGO_FLAGS="--release"
TIMESTAMP=$(date +"%Y%m%d_%H%M%S")

# Parse command line arguments
SKIP_LINUX=false
SKIP_MACOS=false
SKIP_WINDOWS=false
WINDOWS_ONLY=false
INTERACTIVE=false
ALL_PLATFORMS=false

while [[ $# -gt 0 ]]; do
    case $1 in
        --skip-linux)
            SKIP_LINUX=true
            shift
            ;;
        --skip-macos)
            SKIP_MACOS=true
            shift
            ;;
        --skip-windows)
            SKIP_WINDOWS=true
            shift
            ;;
        --windows-only)
            WINDOWS_ONLY=true
            shift
            ;;
        --interactive)
            INTERACTIVE=true
            shift
            ;;
        --all)
            ALL_PLATFORMS=true
            shift
            ;;
        *)
            echo "Unknown option: $1"
            exit 1
            ;;
    esac
done

# Interactive mode if no arguments provided or --interactive
if [ "$INTERACTIVE" = true ] || [ $# -eq 0 ]; then
    echo -e "${YELLOW}📋 Select platforms to build (comma-separated: linux,macos,windows,all):${NC}"
    read -p "> " platforms
    if [[ $platforms == *"all"* ]]; then
        ALL_PLATFORMS=true
    else
        if [[ $platforms == *"linux"* ]]; then SKIP_LINUX=false; else SKIP_LINUX=true; fi
        if [[ $platforms == *"macos"* ]]; then SKIP_MACOS=false; else SKIP_MACOS=true; fi
        if [[ $platforms == *"windows"* ]]; then SKIP_WINDOWS=false; else SKIP_WINDOWS=true; fi
    fi
fi

# Target platforms and architectures
TARGETS=""

# Add targets based on flags
if [ "$WINDOWS_ONLY" = true ]; then
    # Only Windows targets
    TARGETS="$TARGETS windows_amd64:x86_64-pc-windows-msvc"
    TARGETS="$TARGETS windows_arm64:aarch64-pc-windows-msvc"
else
    # Add Windows targets if not explicitly skipped
    if [ "$SKIP_WINDOWS" = false ]; then
        TARGETS="$TARGETS windows_amd64:x86_64-pc-windows-msvc"
        TARGETS="$TARGETS windows_arm64:aarch64-pc-windows-msvc"
    fi
    
    # Add macOS targets if not skipped
    if [ "$SKIP_MACOS" = false ]; then
        TARGETS="$TARGETS macos_amd64:x86_64-apple-darwin"
        TARGETS="$TARGETS macos_arm64:aarch64-apple-darwin"
    fi
    
    # Add Linux targets if not skipped
    if [ "$SKIP_LINUX" = false ]; then
        TARGETS="$TARGETS linux_amd64:x86_64-unknown-linux-gnu"
        TARGETS="$TARGETS linux_arm64:aarch64-unknown-linux-gnu"
    fi
fi

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'
echo -e "${BLUE}🚀 Starting OpenAce Cross-Platform Build${NC}"
echo -e "${BLUE}=========================================${NC}"

# Create build directory structure with timestamp
echo "📁 Creating build directory structure..."
if [ ! -d "$BUILD_DIR" ]; then
    mkdir -p "$BUILD_DIR"
fi

TIMESTAMP_DIR="$BUILD_DIR/$TIMESTAMP"
mkdir -p "$TIMESTAMP_DIR"
echo "📅 Build timestamp: $TIMESTAMP"

# Function to install target if not present
install_target() {
    local target=$1
    echo "🔧 Checking target: $target"
    
    if ! rustup target list --installed | grep -q "^$target$"; then
        echo "📦 Installing target: $target"
        if ! rustup target add "$target"; then
            echo "❌ Failed to install target: $target"
            return 1
        fi
    else
        echo "✅ Target already installed: $target"
    fi
    return 0
}

# Function to build for a specific target
build_target() {
    local platform_arch=$1
    local target=$2
    
    echo -e "${BLUE}🔨 Building for $platform_arch ($target)...${NC}"
    
    # Install target if needed
    if ! install_target "$target"; then
        return 1
    fi
    
    # Set environment variables for static linking
    export RUSTFLAGS="-C target-feature=+crt-static"
    
    # Special handling for Windows
    if [[ $platform_arch == windows_* ]]; then
        export RUSTFLAGS="$RUSTFLAGS -C link-arg=-static"
    fi
    
    # Special handling for Linux static builds
    if [[ $platform_arch == linux_* ]]; then
        export RUSTFLAGS="$RUSTFLAGS -C target-feature=+crt-static -C link-arg=-static"
    fi
    
    # Build the binary
    local build_command="cargo build $CARGO_FLAGS --target=$target"
    echo "Executing: $build_command"
    
    if eval "$build_command"; then
        # Determine binary extension
        local binary_ext=""
        if [[ $platform_arch == windows_* ]]; then
            binary_ext=".exe"
        fi
        
        # Copy binary to output directory with platform-specific name
        local source_binary="target/$target/release/$PROJECT_NAME$binary_ext"
        local dest_binary="$TIMESTAMP_DIR/${PROJECT_NAME}_${platform_arch}$binary_ext"
        
        if [ -f "$source_binary" ]; then
            cp "$source_binary" "$dest_binary"
            
            # Get binary size
            local size=$(du -h "$dest_binary" | cut -f1)
            echo -e "${GREEN}✅ Successfully built $platform_arch ($size)${NC}"
            
            # Strip binary for smaller size (except Windows)
            if [[ $platform_arch != windows_* ]] && command -v strip >/dev/null 2>&1; then
                strip "$dest_binary" 2>/dev/null || true
                local stripped_size=$(du -h "$dest_binary" | cut -f1)
                echo -e "${GREEN}📦 Stripped binary: $stripped_size${NC}"
            fi
            
            return 0
        else
            echo -e "${RED}❌ Binary not found: $source_binary${NC}"
            return 1
        fi
    else
        echo -e "${RED}❌ Build failed for $platform_arch${NC}"
        return 1
    fi
    
    # Reset environment variables
    unset RUSTFLAGS
}

# Build for all targets
echo "🏗️  Building for all platforms..."
echo ""

failed_builds=()
successful_builds=()

for target_pair in $TARGETS; do
    platform_arch="${target_pair%:*}"
    target="${target_pair#*:}"
    
    if build_target "$platform_arch" "$target"; then
        successful_builds+=("$platform_arch")
    else
        failed_builds+=("$platform_arch")
    fi
    echo ""
done

# Summary
echo "📊 Build Summary"
echo "================"

if [ ${#successful_builds[@]} -gt 0 ]; then
    echo "✅ Successful builds (${#successful_builds[@]}):"
    for build in "${successful_builds[@]}"; do
        echo "   • $build"
    done
fi

if [ ${#failed_builds[@]} -gt 0 ]; then
    echo "❌ Failed builds (${#failed_builds[@]}):"
    for build in "${failed_builds[@]}"; do
        echo "   • $build"
    done
fi

echo ""
echo "📁 Build artifacts location: $(pwd)/$TIMESTAMP_DIR"
echo ""

# List all built binaries with sizes
if [ ${#successful_builds[@]} -gt 0 ]; then
    echo "📦 Built binaries:"
    for platform_arch in "${successful_builds[@]}"; do
        binary_ext=""
        if [[ $platform_arch == windows_* ]]; then
            binary_ext=".exe"
        fi
        
        binary_path="$TIMESTAMP_DIR/${PROJECT_NAME}_${platform_arch}$binary_ext"
        
        if [ -f "$binary_path" ]; then
            local size=$(du -m "$binary_path" | cut -f1)
            echo "   ${PROJECT_NAME}_${platform_arch}$binary_ext: ${size} MB"
        fi
    done
fi

echo ""

# Clean build artifacts after completion
echo "🧹 Cleaning build artifacts..."
if cargo clean; then
    echo "✅ Build artifacts cleaned successfully"
else
    echo "⚠️  Failed to clean build artifacts"
fi

echo ""
if [ ${#failed_builds[@]} -eq 0 ]; then
    echo "🎉 All builds completed successfully!"
    exit 0
else
    echo "⚠️  Some builds failed. Check the output above for details."
    exit 1
fi