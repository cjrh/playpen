use std::env;
use std::path::PathBuf;
use std::process::Command;
use tempfile::TempDir;

/// Get the path to the playpen binary
pub fn get_playpen_path() -> PathBuf {
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

/// Run a command and return the output
pub fn run_command(cmd: &mut Command) -> std::process::Output {
    cmd.output().expect("Failed to execute command")
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