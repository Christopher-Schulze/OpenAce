# OpenAce Cross-Platform Build Script (PowerShell)
# Builds static binaries for all supported platforms and architectures

param(
    [switch]$SkipLinux,
    [switch]$SkipMacOS,
    [switch]$WindowsOnly
)

# Build configuration
$ProjectName = "OpenAce-Engine"
$BuildDir = "OpenAce-Builds"
$CargoFlags = "--release"
$Timestamp = Get-Date -Format "yyyyMMdd_HHmmss"

# Target platforms and architectures
$Targets = @{
    "windows_amd64" = "x86_64-pc-windows-msvc"
    "windows_arm64" = "aarch64-pc-windows-msvc"
}

if (-not $WindowsOnly) {
    if (-not $SkipMacOS) {
        $Targets["macos_amd64"] = "x86_64-apple-darwin"
        $Targets["macos_arm64"] = "aarch64-apple-darwin"
    }
    if (-not $SkipLinux) {
        $Targets["linux_amd64"] = "x86_64-unknown-linux-gnu"
        $Targets["linux_arm64"] = "aarch64-unknown-linux-gnu"
    }
}

Write-Host "🚀 Starting OpenAce Cross-Platform Build" -ForegroundColor Blue
Write-Host "=========================================" -ForegroundColor Blue

# Create build directory structure with timestamp
Write-Host "📁 Creating build directory structure..." -ForegroundColor Yellow
if (-not (Test-Path $BuildDir)) {
    New-Item -ItemType Directory -Path $BuildDir | Out-Null
}

$TimestampDir = "$BuildDir\$Timestamp"
New-Item -ItemType Directory -Path $TimestampDir | Out-Null
Write-Host "📅 Build timestamp: $Timestamp" -ForegroundColor Cyan

# Function to install target if not present
function Install-Target {
    param([string]$Target)
    
    Write-Host "🔧 Checking target: $Target" -ForegroundColor Yellow
    $installedTargets = rustup target list --installed
    if ($installedTargets -notcontains $Target) {
        Write-Host "📦 Installing target: $Target" -ForegroundColor Yellow
        rustup target add $Target
        if ($LASTEXITCODE -ne 0) {
            Write-Host "❌ Failed to install target: $Target" -ForegroundColor Red
            return $false
        }
    } else {
        Write-Host "✅ Target already installed: $Target" -ForegroundColor Green
    }
    return $true
}

# Function to build for a specific target
function Build-Target {
    param(
        [string]$PlatformArch,
        [string]$Target
    )
    
    $outputDir = $TimestampDir
    
    Write-Host "🔨 Building for $PlatformArch ($Target)..." -ForegroundColor Blue
    
    # Install target if needed
    if (-not (Install-Target $Target)) {
        return $false
    }
    
    # Set environment variables for static linking
    $env:RUSTFLAGS = "-C target-feature=+crt-static"
    
    # Special handling for Windows
    if ($PlatformArch -like "windows_*") {
        $env:RUSTFLAGS += " -C link-arg=/SUBSYSTEM:CONSOLE"
    }
    
    # Build the binary
    $buildCommand = "cargo build $CargoFlags --target=$Target"
    Write-Host "Executing: $buildCommand" -ForegroundColor Gray
    
    Invoke-Expression $buildCommand
    
    if ($LASTEXITCODE -eq 0) {
        # Determine binary extension
        $binaryExt = ""
        if ($PlatformArch -like "windows_*") {
            $binaryExt = ".exe"
        }
        
        # Copy binary to output directory with platform-specific name
        $sourceBinary = "target\$Target\release\$ProjectName$binaryExt"
        $destBinary = "$outputDir\$ProjectName`_$PlatformArch$binaryExt"
        
        if (Test-Path $sourceBinary) {
            Copy-Item $sourceBinary $destBinary
            
            # Get binary size
            $size = [math]::Round((Get-Item $destBinary).Length / 1MB, 2)
            Write-Host "✅ Successfully built $PlatformArch ($size MB)" -ForegroundColor Green
            
            return $true
        } else {
            Write-Host "❌ Binary not found: $sourceBinary" -ForegroundColor Red
            return $false
        }
    } else {
        Write-Host "❌ Build failed for $PlatformArch" -ForegroundColor Red
        return $false
    }
    
    # Reset environment variables
    Remove-Item Env:RUSTFLAGS -ErrorAction SilentlyContinue
}

# Build for all targets
Write-Host "🏗️  Building for all platforms..." -ForegroundColor Blue
Write-Host ""

$failedBuilds = @()
$successfulBuilds = @()

foreach ($platformArch in $Targets.Keys) {
    $target = $Targets[$platformArch]
    
    if (Build-Target $platformArch $target) {
        $successfulBuilds += $platformArch
    } else {
        $failedBuilds += $platformArch
    }
    Write-Host ""
}

# Summary
Write-Host "📊 Build Summary" -ForegroundColor Blue
Write-Host "================" -ForegroundColor Blue

if ($successfulBuilds.Count -gt 0) {
    Write-Host "✅ Successful builds ($($successfulBuilds.Count)):" -ForegroundColor Green
    foreach ($build in $successfulBuilds) {
        Write-Host "   • $build" -ForegroundColor Green
    }
}

if ($failedBuilds.Count -gt 0) {
    Write-Host "❌ Failed builds ($($failedBuilds.Count)):" -ForegroundColor Red
    foreach ($build in $failedBuilds) {
        Write-Host "   • $build" -ForegroundColor Red
    }
}

Write-Host ""
Write-Host "📁 Build artifacts location: $(Get-Location)\$TimestampDir" -ForegroundColor Blue
Write-Host ""

# List all built binaries with sizes
if ($successfulBuilds.Count -gt 0) {
    Write-Host "📦 Built binaries:" -ForegroundColor Blue
    foreach ($platformArch in $successfulBuilds) {
        $binaryExt = ""
        if ($platformArch -like "windows_*") {
            $binaryExt = ".exe"
        }
        
        $binaryPath = "$TimestampDir\$ProjectName`_$platformArch$binaryExt"
        
        if (Test-Path $binaryPath) {
            $size = [math]::Round((Get-Item $binaryPath).Length / 1MB, 2)
            Write-Host "   $ProjectName`_$platformArch$binaryExt`: $size MB" -ForegroundColor Green
        }
    }
}

# Clean build artifacts
Write-Host "🧹 Cleaning build artifacts..." -ForegroundColor Yellow
cargo clean
Write-Host "✨ Build artifacts cleaned" -ForegroundColor Green

Write-Host ""
if ($failedBuilds.Count -eq 0) {
    Write-Host "🎉 All builds completed successfully!" -ForegroundColor Green
    exit 0
} else {
    Write-Host "⚠️  Some builds failed. Check the output above for details." -ForegroundColor Yellow
    exit 1
}