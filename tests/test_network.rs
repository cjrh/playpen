use assert_cmd::Command;
use predicates::prelude::*;

mod common;

// ============ --private-network flag ============

#[test]
fn test_private_network_true() {
    let mut cmd = Command::new(common::get_playpen_path());
    cmd.args(["--private-network", "true", "--dry-run", "--", "echo", "hi"]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("-pPrivateNetwork=yes"));
}

#[test]
fn test_private_network_false() {
    let mut cmd = Command::new(common::get_playpen_path());
    cmd.args(["--private-network", "false", "--dry-run", "--", "echo", "hi"]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("-pPrivateNetwork=no"));
}

#[test]
fn test_private_network_boolish_yes() {
    let mut cmd = Command::new(common::get_playpen_path());
    cmd.args(["--private-network", "yes", "--dry-run", "--", "echo", "hi"]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("-pPrivateNetwork=yes"));
}

#[test]
fn test_no_private_network_no_profile_absent() {
    let mut cmd = Command::new(common::get_playpen_path());
    cmd.args(["--dry-run", "--", "echo", "hi"]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("PrivateNetwork").not());
}

#[test]
fn test_private_network_with_profile_after() {
    let mut cmd = Command::new(common::get_playpen_path());
    cmd.args([
        "--private-network",
        "true",
        "--profile",
        "cargo",
        "--dry-run",
        "--",
        "echo",
        "hi",
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("-pPrivateNetwork=yes"));
}

#[test]
fn test_private_network_with_profile_before() {
    // Same result as test_private_network_with_profile_after — order-independent.
    let mut cmd = Command::new(common::get_playpen_path());
    cmd.args([
        "--profile",
        "cargo",
        "--private-network",
        "true",
        "--dry-run",
        "--",
        "echo",
        "hi",
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("-pPrivateNetwork=yes"));
}

// ============ Profile network defaults (v1: never emit PrivateNetwork) ============

#[test]
fn test_profile_cargo_no_private_network() {
    let mut cmd = Command::new(common::get_playpen_path());
    cmd.args(["--profile", "cargo", "--dry-run", "--", "echo", "hi"]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("PrivateNetwork").not());
}

#[test]
fn test_profile_coding_agent_no_private_network() {
    let temp_home = common::create_temp_dir();
    let mut cmd = Command::new(common::get_playpen_path());
    cmd.env("HOME", temp_home.path());
    cmd.args(["--profile", "coding-agent", "--dry-run", "--", "claude"]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("PrivateNetwork").not());
}

#[test]
fn test_profile_shell_no_private_network() {
    let temp_home = common::create_temp_dir();
    let mut cmd = Command::new(common::get_playpen_path());
    cmd.env("HOME", temp_home.path());
    cmd.args(["--profile", "shell", "--dry-run", "--", "bash"]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("PrivateNetwork").not());
}

// ============ IP filtering ============

#[test]
fn test_ip_allow_single() {
    let mut cmd = Command::new(common::get_playpen_path());
    cmd.args(["--ip-allow", "127.0.0.1", "--dry-run", "--", "echo", "hi"]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("-pIPAddressAllow=127.0.0.1"));
}

#[test]
fn test_ip_deny_any() {
    let mut cmd = Command::new(common::get_playpen_path());
    cmd.args(["--ip-deny", "any", "--dry-run", "--", "echo", "hi"]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("-pIPAddressDeny=any"));
}

#[test]
fn test_ip_allow_multiple() {
    let mut cmd = Command::new(common::get_playpen_path());
    cmd.args([
        "--ip-allow",
        "127.0.0.1",
        "--ip-allow",
        "10.0.0.0/8",
        "--dry-run",
        "--",
        "echo",
        "hi",
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("-pIPAddressAllow=127.0.0.1"))
        .stdout(predicate::str::contains("-pIPAddressAllow=10.0.0.0/8"));
}

#[test]
fn test_ip_deny_multiple() {
    let mut cmd = Command::new(common::get_playpen_path());
    cmd.args([
        "--ip-deny",
        "10.0.0.0/8",
        "--ip-deny",
        "192.168.0.0/16",
        "--dry-run",
        "--",
        "echo",
        "hi",
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("-pIPAddressDeny=10.0.0.0/8"))
        .stdout(predicate::str::contains("-pIPAddressDeny=192.168.0.0/16"));
}

#[test]
fn test_ip_allow_and_deny_together() {
    let mut cmd = Command::new(common::get_playpen_path());
    cmd.args([
        "--ip-deny",
        "any",
        "--ip-allow",
        "localhost",
        "--dry-run",
        "--",
        "echo",
        "hi",
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("-pIPAddressDeny=any"))
        .stdout(predicate::str::contains("-pIPAddressAllow=localhost"));
}

// ============ Socket bind filtering ============

#[test]
fn test_socket_bind_deny_any() {
    let mut cmd = Command::new(common::get_playpen_path());
    cmd.args(["--socket-bind-deny", "any", "--dry-run", "--", "echo", "hi"]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("-pSocketBindDeny=any"));
}

#[test]
fn test_socket_bind_allow_port() {
    let mut cmd = Command::new(common::get_playpen_path());
    cmd.args(["--socket-bind-allow", "8080", "--dry-run", "--", "echo", "hi"]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("-pSocketBindAllow=8080"));
}

#[test]
fn test_socket_bind_allow_multiple() {
    let mut cmd = Command::new(common::get_playpen_path());
    cmd.args([
        "--socket-bind-allow",
        "8080",
        "--socket-bind-allow",
        "9090",
        "--dry-run",
        "--",
        "echo",
        "hi",
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("-pSocketBindAllow=8080"))
        .stdout(predicate::str::contains("-pSocketBindAllow=9090"));
}

#[test]
fn test_socket_bind_deny_multiple() {
    let mut cmd = Command::new(common::get_playpen_path());
    cmd.args([
        "--socket-bind-deny",
        "tcp:80",
        "--socket-bind-deny",
        "udp:53",
        "--dry-run",
        "--",
        "echo",
        "hi",
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("-pSocketBindDeny=tcp:80"))
        .stdout(predicate::str::contains("-pSocketBindDeny=udp:53"));
}

#[test]
fn test_socket_bind_allow_and_deny_together() {
    let mut cmd = Command::new(common::get_playpen_path());
    cmd.args([
        "--socket-bind-allow",
        "8080",
        "--socket-bind-deny",
        "any",
        "--dry-run",
        "--",
        "echo",
        "hi",
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("-pSocketBindAllow=8080"))
        .stdout(predicate::str::contains("-pSocketBindDeny=any"));
}

#[test]
fn test_socket_bind_complex_rule_verbatim() {
    let mut cmd = Command::new(common::get_playpen_path());
    cmd.args([
        "--socket-bind-allow",
        "ipv4:tcp:8080",
        "--dry-run",
        "--",
        "echo",
        "hi",
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("-pSocketBindAllow=ipv4:tcp:8080"));
}

// ============ Combination tests ============

#[test]
fn test_private_network_plus_ip_deny() {
    let mut cmd = Command::new(common::get_playpen_path());
    cmd.args([
        "--private-network",
        "true",
        "--ip-deny",
        "any",
        "--dry-run",
        "--",
        "echo",
        "hi",
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("-pPrivateNetwork=yes"))
        .stdout(predicate::str::contains("-pIPAddressDeny=any"));
}

#[test]
fn test_ip_allow_list_firewall_without_namespace() {
    let mut cmd = Command::new(common::get_playpen_path());
    cmd.args([
        "--private-network",
        "false",
        "--ip-deny",
        "any",
        "--ip-allow",
        "localhost",
        "--dry-run",
        "--",
        "echo",
        "hi",
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("-pPrivateNetwork=no"))
        .stdout(predicate::str::contains("-pIPAddressDeny=any"))
        .stdout(predicate::str::contains("-pIPAddressAllow=localhost"));
}

#[test]
fn test_socket_bind_deny_with_profile() {
    let mut cmd = Command::new(common::get_playpen_path());
    cmd.args([
        "--socket-bind-deny",
        "any",
        "--profile",
        "cargo",
        "--dry-run",
        "--",
        "echo",
        "hi",
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("-pSocketBindDeny=any"));
}
