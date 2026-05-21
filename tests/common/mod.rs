// This file is included via `mod common;` in every integration test binary.
// A helper used by only one of those binaries looks "never used" to the
// others, so dead-code warnings here are unavoidable false positives.
#![allow(dead_code)]

use std::env;
use std::path::PathBuf;
use std::process::Command;
use tempfile::TempDir;

/// Get the path to the playpen binary
pub fn get_playpen_path() -> PathBuf {
    // Cargo sets this env var for integration tests
    if let Ok(path) = env::var("CARGO_BIN_EXE_playpen") {
        return PathBuf::from(path);
    }

    // Fallback: compute from current_exe location
    let mut path = env::current_exe()
        .expect("Failed to get current executable path")
        .parent()
        .expect("Failed to get parent directory")
        .to_path_buf();

    // Remove the deps directory if present
    if path.file_name().and_then(|s| s.to_str()) == Some("deps") {
        path = path.parent().expect("Failed to get parent").to_path_buf();
    }

    path.push("playpen");
    path
}

/// Create a temporary directory for testing
pub fn create_temp_dir() -> TempDir {
    TempDir::new().expect("Failed to create temporary directory")
}

/// Check if npm is available on the system
pub fn npm_available() -> bool {
    Command::new("npm")
        .arg("--version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

/// Check if node is available on the system
pub fn node_available() -> bool {
    Command::new("node")
        .arg("--version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}
