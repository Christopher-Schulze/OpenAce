//! Build script for OpenAce Engine
//!
//! This script configures the build process to include OS and platform information
//! in the binary name and sets up conditional compilation flags.

use std::env;
use std::fs;
use std::path::Path;

fn main() {
    // Get target information
    let target = env::var("TARGET").unwrap();
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap();
    let target_arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap();
    let target_env = env::var("CARGO_CFG_TARGET_ENV").unwrap_or_else(|_| "unknown".to_string());
    
    // Set build-time environment variables
    println!("cargo:rustc-env=TARGET={}", target);
    println!("cargo:rustc-env=BUILD_TARGET_OS={}", target_os);
    println!("cargo:rustc-env=BUILD_TARGET_ARCH={}", target_arch);
    println!("cargo:rustc-env=BUILD_TARGET_ENV={}", target_env);
    
    // Create platform-specific binary name
    let binary_suffix = format!("{}-{}-{}", target_os, target_arch, target_env);
    println!("cargo:rustc-env=BINARY_PLATFORM_SUFFIX={}", binary_suffix);
    
    // Set conditional compilation flags based on target
    match target_os.as_str() {
        "windows" => {
            println!("cargo:rustc-cfg=target_windows");
            if target_arch == "x86_64" {
                println!("cargo:rustc-cfg=target_windows_x64");
            }
        }
        "macos" => {
            println!("cargo:rustc-cfg=target_macos");
            if target_arch == "aarch64" {
                println!("cargo:rustc-cfg=target_macos_arm64");
            } else if target_arch == "x86_64" {
                println!("cargo:rustc-cfg=target_macos_x64");
            }
        }
        "linux" => {
            println!("cargo:rustc-cfg=target_linux");
            if target_arch == "x86_64" {
                println!("cargo:rustc-cfg=target_linux_x64");
            } else if target_arch == "aarch64" {
                println!("cargo:rustc-cfg=target_linux_arm64");
            }
        }
        _ => {
            println!("cargo:rustc-cfg=target_other");
        }
    }
    
    // Generate build information
    generate_build_info(&target, &target_os, &target_arch, &target_env);
    
    // Rerun if build script changes
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=Cargo.toml");
}

fn generate_build_info(target: &str, target_os: &str, target_arch: &str, target_env: &str) {
    let out_dir = env::var("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("build_info.rs");
    
    let build_info = format!(
        r#"
/// Build-time information about the binary
pub mod build_info {{
    /// Full target triple
    pub const TARGET: &str = "{}";
    
    /// Target operating system
    pub const TARGET_OS: &str = "{}";
    
    /// Target architecture
    pub const TARGET_ARCH: &str = "{}";
    
    /// Target environment
    pub const TARGET_ENV: &str = "{}";
    
    /// Platform suffix for binary name
    pub const PLATFORM_SUFFIX: &str = "{}-{}-{}";
    
    /// Full binary name with platform information
    pub const BINARY_NAME: &str = "OpenAce-Engine-{}-{}-{}";
    
    /// Build timestamp
    pub const BUILD_TIMESTAMP: &str = "{}";
    
    /// Rust version used for build
    pub const RUSTC_VERSION: &str = "{}";
}}
"#,
        target,
        target_os,
        target_arch,
        target_env,
        target_os,
        target_arch,
        target_env,
        target_os,
        target_arch,
        target_env,
        env::var("BUILD_TIMESTAMP").unwrap_or_else(|_| chrono::Utc::now().to_rfc3339()),
        env::var("RUSTC_VERSION").unwrap_or_else(|_| "unknown".to_string())
    );
    
    fs::write(&dest_path, build_info).unwrap();
}