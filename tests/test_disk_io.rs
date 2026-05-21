use assert_cmd::Command;
use predicates::prelude::*;

mod common;

// All tests use --dry-run, so they inspect the rendered systemd-run command
// without needing a real cgroup. The disk-I/O properties carry the block
// device backing the working directory, which dry-run renders as the test's
// current directory; the assertions match only the property name and rate.

// ============ --disk-limit (covers both directions) ============

#[test]
fn test_disk_limit_sets_read_and_write() {
    let mut cmd = Command::new(common::get_playpen_path());
    cmd.args(["-d", "50M", "--dry-run", "--", "echo", "hi"]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("-pIOReadBandwidthMax="))
        .stdout(predicate::str::contains("-pIOWriteBandwidthMax="))
        // Both directions get the same rate from --disk-limit.
        .stdout(predicate::str::is_match(r"IOReadBandwidthMax=\S+ 50M").unwrap())
        .stdout(predicate::str::is_match(r"IOWriteBandwidthMax=\S+ 50M").unwrap());
}

#[test]
fn test_disk_limit_long_flag() {
    let mut cmd = Command::new(common::get_playpen_path());
    cmd.args(["--disk-limit", "500K", "--dry-run", "--", "echo", "hi"]);

    cmd.assert()
        .success()
        .stdout(predicate::str::is_match(r"IOReadBandwidthMax=\S+ 500K").unwrap())
        .stdout(predicate::str::is_match(r"IOWriteBandwidthMax=\S+ 500K").unwrap());
}

// ============ Single-direction flags ============

#[test]
fn test_disk_read_only() {
    let mut cmd = Command::new(common::get_playpen_path());
    cmd.args(["--disk-read", "10M", "--dry-run", "--", "echo", "hi"]);

    cmd.assert()
        .success()
        .stdout(predicate::str::is_match(r"IOReadBandwidthMax=\S+ 10M").unwrap())
        // Write stays unlimited when only --disk-read is given.
        .stdout(predicate::str::contains("IOWriteBandwidthMax").not());
}

#[test]
fn test_disk_write_only() {
    let mut cmd = Command::new(common::get_playpen_path());
    cmd.args(["--disk-write", "5M", "--dry-run", "--", "echo", "hi"]);

    cmd.assert()
        .success()
        .stdout(predicate::str::is_match(r"IOWriteBandwidthMax=\S+ 5M").unwrap())
        .stdout(predicate::str::contains("IOReadBandwidthMax").not());
}

// ============ Override precedence ============

#[test]
fn test_disk_read_overrides_disk_limit() {
    // --disk-limit seeds both directions; --disk-read then overrides reads
    // only, leaving writes at the --disk-limit value.
    let mut cmd = Command::new(common::get_playpen_path());
    cmd.args(["-d", "50M", "--disk-read", "10M", "--dry-run", "--", "echo", "hi"]);

    cmd.assert()
        .success()
        .stdout(predicate::str::is_match(r"IOReadBandwidthMax=\S+ 10M").unwrap())
        .stdout(predicate::str::is_match(r"IOWriteBandwidthMax=\S+ 50M").unwrap());
}

#[test]
fn test_disk_write_overrides_disk_limit() {
    let mut cmd = Command::new(common::get_playpen_path());
    cmd.args(["-d", "50M", "--disk-write", "8M", "--dry-run", "--", "echo", "hi"]);

    cmd.assert()
        .success()
        .stdout(predicate::str::is_match(r"IOReadBandwidthMax=\S+ 50M").unwrap())
        .stdout(predicate::str::is_match(r"IOWriteBandwidthMax=\S+ 8M").unwrap());
}

// ============ Absence ============

#[test]
fn test_no_disk_flags_no_io_properties() {
    let mut cmd = Command::new(common::get_playpen_path());
    cmd.args(["--dry-run", "--", "echo", "hi"]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("IOReadBandwidthMax").not())
        .stdout(predicate::str::contains("IOWriteBandwidthMax").not());
}

// ============ Combination with a profile ============

#[test]
fn test_disk_limit_with_profile() {
    // A disk limit is orthogonal to a profile: the profile's memory/CPU
    // limits remain and the disk properties are added alongside.
    let mut cmd = Command::new(common::get_playpen_path());
    cmd.args(["--profile", "cargo", "-d", "20M", "--dry-run", "--", "echo", "hi"]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("-pMemoryMax=2G"))
        .stdout(predicate::str::is_match(r"IOReadBandwidthMax=\S+ 20M").unwrap())
        .stdout(predicate::str::is_match(r"IOWriteBandwidthMax=\S+ 20M").unwrap());
}
