use assert_cmd::Command;
use predicates::prelude::*;

mod common;

#[test]
fn test_cargo_profile_dry_run() {
    let mut cmd = Command::new(common::get_playpen_path());
    cmd.args(["--profile", "cargo", "--dry-run", "--", "echo", "hello"]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("-pMemoryMax=2G"))
        .stdout(predicate::str::contains("-pMemorySwapMax=0"))
        .stdout(predicate::str::contains("-pCPUQuota=300%"))
        .stdout(predicate::str::contains("-pProtectHome=tmpfs"))
        .stdout(predicate::str::contains("-pBindPaths=").and(predicate::str::contains("/.cargo")))
        .stdout(
            predicate::str::contains("-pBindReadOnlyPaths=")
                .and(predicate::str::contains("/.rustup")),
        );
}

#[test]
fn test_go_profile_dry_run() {
    let mut cmd = Command::new(common::get_playpen_path());
    cmd.args(["--profile", "go", "--dry-run", "--", "echo", "hello"]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("-pMemoryMax=512M"))
        .stdout(predicate::str::contains("-pCPUQuota=300%"))
        .stdout(predicate::str::contains("-pProtectHome=tmpfs"));

    let output = cmd.output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    // Go profile should NOT set MemorySwapMax
    assert!(
        !stdout.contains("-pMemorySwapMax"),
        "Go profile should not set MemorySwapMax"
    );
}

#[test]
fn test_shell_profile_dry_run() {
    // Create a fake home directory with the paths the shell profile expects
    let temp_home = common::create_temp_dir();
    std::fs::create_dir_all(temp_home.path().join(".local/share")).unwrap();
    std::fs::create_dir_all(temp_home.path().join(".cache")).unwrap();
    std::fs::create_dir_all(temp_home.path().join(".local/bin")).unwrap();

    let mut cmd = Command::new(common::get_playpen_path());
    cmd.env("HOME", temp_home.path());
    cmd.args(["--profile", "shell", "--dry-run", "--", "bash"]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("-pMemoryMax=4G"))
        .stdout(predicate::str::contains("-pMemorySwapMax=0"))
        .stdout(predicate::str::contains("-pProtectHome=read-only"));

    let output = cmd.output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    // Shell profile should NOT set CPUQuota
    assert!(
        !stdout.contains("-pCPUQuota"),
        "Shell profile should not set CPUQuota"
    );
    // Shell profile should include rw_paths
    assert!(stdout.contains("-pBindPaths="));
    assert!(stdout.contains("/.local/share"));
    assert!(stdout.contains("/.cache"));
    assert!(stdout.contains("/.local/bin"));
}

#[test]
fn test_coding_agent_profile_dry_run() {
    // Create a fake home directory with the paths the coding-agent profile expects
    let temp_home = common::create_temp_dir();
    std::fs::write(temp_home.path().join(".gitconfig"), "[user]\nname = test\n").unwrap();
    std::fs::create_dir(temp_home.path().join(".ssh")).unwrap();

    let mut cmd = Command::new(common::get_playpen_path());
    cmd.env("HOME", temp_home.path());
    cmd.args(["--profile", "coding-agent", "--dry-run", "--", "claude"]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("-pMemoryMax=4G"))
        .stdout(predicate::str::contains("-pCPUQuota=200%"))
        .stdout(predicate::str::contains("-pProtectHome=tmpfs"))
        .stdout(
            predicate::str::contains("-pBindReadOnlyPaths=")
                .and(predicate::str::contains("/.gitconfig")),
        )
        .stdout(
            predicate::str::contains("-pBindReadOnlyPaths=").and(predicate::str::contains("/.ssh")),
        );
}

#[test]
fn test_explicit_after_profile_overrides() {
    let mut cmd = Command::new(common::get_playpen_path());
    cmd.args([
        "--profile",
        "cargo",
        "-m",
        "4G",
        "--dry-run",
        "--",
        "echo",
        "hello",
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("-pMemoryMax=4G"));
}

#[test]
fn test_explicit_overrides_profile_regardless_of_order() {
    // An explicit flag beats the profile even when it appears *before*
    // --profile on the command line; argument order does not matter.
    let mut cmd = Command::new(common::get_playpen_path());
    cmd.args([
        "-m",
        "4G",
        "--profile",
        "cargo",
        "--dry-run",
        "--",
        "echo",
        "hello",
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("-pMemoryMax=4G"));
}

#[test]
fn test_memory_swap_max_override() {
    // Go profile allows swap, but --memory-swap-max 0 overrides
    let mut cmd = Command::new(common::get_playpen_path());
    cmd.args([
        "--profile",
        "go",
        "--memory-swap-max",
        "0",
        "--dry-run",
        "--",
        "echo",
        "hello",
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("-pMemorySwapMax=0"));
}

#[test]
fn test_profile_plus_explicit_rw_accumulates() {
    let mut cmd = Command::new(common::get_playpen_path());
    cmd.args([
        "--profile",
        "cargo",
        "--rw",
        "/tmp/extra",
        "--dry-run",
        "--",
        "echo",
        "hello",
    ]);

    let output = cmd.output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Should have both profile paths and explicit paths
    assert!(stdout.contains("/.cargo"), "Should contain profile rw path");
    assert!(
        stdout.contains("/tmp/extra"),
        "Should contain explicit rw path"
    );
}

#[test]
fn test_current_dir_only_overrides_profile_regardless_of_order() {
    // --current-dir-only always applies its tmpfs lockdown, overriding the
    // profile's protect_home no matter which side of --profile it appears on.
    for args in [
        ["--profile", "shell", "--current-dir-only"],
        ["--current-dir-only", "--profile", "shell"],
    ] {
        let mut cmd = Command::new(common::get_playpen_path());
        cmd.args(args).args(["--dry-run", "--", "bash"]);

        cmd.assert()
            .success()
            .stdout(predicate::str::contains("-pProtectHome=tmpfs"));
    }
}

#[test]
fn test_unknown_profile_error() {
    let mut cmd = Command::new(common::get_playpen_path());
    cmd.args([
        "--profile",
        "nonexistent",
        "--dry-run",
        "--",
        "echo",
        "hello",
    ]);

    cmd.assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains(
            "error: unknown profile 'nonexistent'",
        ))
        .stderr(predicate::str::contains("cargo"))
        .stderr(predicate::str::contains("shell"));
}

#[test]
fn test_no_profile_no_memory_swap_max() {
    let mut cmd = Command::new(common::get_playpen_path());
    cmd.args(["-m", "50M", "--dry-run", "--", "echo", "hello"]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("-pMemoryMax=50M"))
        .stdout(predicate::str::contains("-pMemorySwapMax=0"));
}

#[test]
fn test_nonexistent_profile_path_skipped() {
    let mut cmd = Command::new(common::get_playpen_path());
    cmd.args(["--profile", "pytest", "--dry-run", "--", "echo", "hello"]);

    let output = cmd.output().unwrap();

    // pytest profile has ro_paths=["$HOME/.local/lib"] which may or may not exist
    // If it exists, it should be in the output; if not, it should be silently skipped
    // We can't assert exact presence, but the command should succeed
    assert!(output.status.success());
}

#[test]
fn test_help_contains_profiles() {
    let mut cmd = Command::new(common::get_playpen_path());
    cmd.arg("--help");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("--profile"))
        .stdout(predicate::str::contains("cargo").or(predicate::str::contains("Valid profiles")));
}
