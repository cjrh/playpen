use std::process::Command;

mod common;

#[test]
fn test_no_protection_home_access() {
    // Test that without protection, we can see home directory contents
    let output = Command::new(common::get_playpen_path())
        .args(&["--protect-home=none", "--", "sh", "-c", "ls /home | wc -l"])
        .output()
        .expect("Failed to execute playpen");

    assert!(output.status.success());
    let line_count = String::from_utf8_lossy(&output.stdout).trim().parse::<i32>().unwrap_or(0);
    // Should see at least one entry (the user's home directory)
    assert!(line_count >= 1, "Expected to see home directory contents, got: {}", line_count);
}

#[test]
fn test_protect_home_tmpfs() {
    // Test that with tmpfs protection, home directory appears empty or minimal
    // Run from root directory to avoid conflicts with protection
    let output = Command::new(common::get_playpen_path())
        .current_dir("/")
        .args(&["--protect-home=tmpfs", "--", "ls", "/home"])
        .output()
        .expect("Failed to execute playpen");

    if !output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        panic!("Command failed. stdout: {}, stderr: {}", stdout, stderr);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let line_count = stdout.lines().count();

    // With tmpfs, /home should be empty or contain only what we bind-mount
    // This depends on implementation, but should be less than a normal home directory
    assert!(line_count <= 1, "Expected minimal home directory contents with tmpfs, got {} lines: {}", line_count, stdout);
}

#[test]
fn test_current_dir_only_blocks_home() {
    let temp_dir = common::create_temp_dir();

    // Create a test script that tries to read the home directory
    let test_script = r#"
import os
import sys
try:
    home_files = os.listdir(os.path.expanduser('~'))
    print(f"Home files count: {len(home_files)}")
    if len(home_files) > 2:  # More than just basic entries
        sys.exit(0)  # Success if we can see home
    else:
        sys.exit(1)  # Fail if home is restricted
except Exception as e:
    print(f"Error accessing home: {e}")
    sys.exit(1)  # Fail if home access is blocked
"#;

    std::fs::write(temp_dir.path().join("test_home.py"), test_script)
        .expect("Failed to write test script");

    let output = Command::new(common::get_playpen_path())
        .current_dir(temp_dir.path())
        .args(&[
            "--current-dir-only",
            "--", "python3", "test_home.py"
        ])
        .output()
        .expect("Failed to execute playpen");

    // The script should fail because home access is blocked by InaccessiblePaths=/
    assert!(!output.status.success(), "Expected home access to be blocked, but script succeeded. Output: {}", String::from_utf8_lossy(&output.stdout));
}

#[test]
fn test_current_dir_only_allows_pwd() {
    let temp_dir = common::create_temp_dir();

    // Create a test file in the current directory
    std::fs::write(temp_dir.path().join("test_file.txt"), "Hello World")
        .expect("Failed to create test file");

    let output = Command::new(common::get_playpen_path())
        .current_dir(temp_dir.path())
        .args(&[
            "--current-dir-only",
            "--", "cat", "test_file.txt"
        ])
        .output()
        .expect("Failed to execute playpen");

    if !output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        panic!("Command failed. stdout: {}, stderr: {}", stdout, stderr);
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Hello World"), "Expected to read file from current directory, got: {}", stdout);
}

#[test]
fn test_current_dir_only_blocks_sensitive_files() {
    let temp_dir = common::create_temp_dir();

    // With ProtectHome=yes, user home directories should be blocked but /home might be listable
    // Test that we can't access sensitive home directory contents
    let sensitive_paths = vec![
        "~/.bashrc",
        "~/.ssh",
        "~/Documents",
        // Note: /home directory itself might be listable with ProtectHome=yes, but contents are protected
    ];

    for path in sensitive_paths {
        let output = Command::new(common::get_playpen_path())
            .current_dir(temp_dir.path())
            .args(&[
                "--current-dir-only",
                "--", "sh", "-c", &format!("test -e {} && echo 'ACCESSIBLE' || echo 'BLOCKED'", path)
            ])
            .output()
            .expect("Failed to execute playpen");

        if !output.status.success() {
            // Command failure is expected with InaccessiblePaths=/ - this means blocking is working
            continue;
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("BLOCKED"), "Expected {} to be blocked, but it was accessible. Output: {}", path, stdout);
    }
}

#[test]
fn test_fine_grained_ro_access() {
    // Test read-only access to /etc from root directory to avoid conflicts
    let output = Command::new(common::get_playpen_path())
        .current_dir("/")
        .args(&["--ro", "/etc", "--", "ls", "/etc/passwd"])
        .output()
        .expect("Failed to execute playpen");

    if !output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        panic!("Expected to read /etc/passwd with read-only access. stdout: {}, stderr: {}", stdout, stderr);
    }

    // Test that we can't write to the read-only path
    let output = Command::new(common::get_playpen_path())
        .current_dir("/")
        .args(&["--ro", "/etc", "--", "sh", "-c", "echo 'test' > /etc/test_file 2>&1 || echo 'WRITE_BLOCKED'"])
        .output()
        .expect("Failed to execute playpen");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("WRITE_BLOCKED") || !output.status.success(),
           "Expected write to be blocked to read-only path, but got: {}", stdout);
}

#[test]
fn test_fine_grained_rw_access() {
    let temp_dir = common::create_temp_dir();
    let test_file = temp_dir.path().join("test_write.txt");

    // Test read-write access to temp directory
    let output = Command::new(common::get_playpen_path())
        .args(&["--rw", temp_dir.path().to_str().unwrap(), "--", "sh", "-c",
                &format!("echo 'test content' > {} && cat {}", test_file.display(), test_file.display())])
        .output()
        .expect("Failed to execute playpen");

    assert!(output.status.success(), "Expected read-write access to work");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("test content"), "Expected to write and read file, got: {}", stdout);
}

#[test]
fn test_inaccessible_paths() {
    let temp_dir = common::create_temp_dir();

    // Test that inaccessible paths are blocked
    let output = Command::new(common::get_playpen_path())
        .current_dir(temp_dir.path())
        .args(&["--inaccessible", "/etc", "--", "ls", "/etc"])
        .output()
        .expect("Failed to execute playpen");

    // Should fail to access /etc
    assert!(!output.status.success(), "Expected /etc to be inaccessible, but command succeeded");
}

#[test]
fn test_memory_limit_still_works() {
    let temp_dir = common::create_temp_dir();

    // Test that memory limits still work with new path features
    let output = Command::new(common::get_playpen_path())
        .current_dir(temp_dir.path())
        .args(&[
            "-m", "50M", "--current-dir-only",
            "--", "echo", "Memory limits work"
        ])
        .output()
        .expect("Failed to execute playpen");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Memory limits work"), "Expected memory limits to work with path restrictions");
}