use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use std::os::unix::fs::{symlink, PermissionsExt};
use std::path::Path;
use tempfile::TempDir;

mod common;

struct Workspace {
    home: TempDir,
}

impl Workspace {
    fn new() -> Self {
        let home = common::create_temp_dir();
        fs::create_dir(home.path().join("project")).unwrap();
        Self { home }
    }

    fn command(&self) -> Command {
        let mut cmd = Command::new(common::get_playpen_path());
        cmd.env_clear()
            .env("HOME", self.home.path())
            .env("PATH", "/usr/bin:/bin")
            .current_dir(self.home.path().join("project"));
        cmd
    }

    fn dry_run(&self) -> Command {
        let mut cmd = self.command();
        cmd.args(["--profile", "coding-agent", "--dry-run", "--", "echo", "ok"]);
        cmd
    }

    fn directory(&self, name: &str) {
        fs::create_dir_all(self.home.path().join(name)).unwrap();
    }
}

#[test]
fn detects_existing_agent_state_without_unrelated_credentials() {
    let w = Workspace::new();
    for path in [
        ".claude",
        ".codex",
        ".gemini",
        ".pi/agent",
        ".pi/context-mode",
        ".pi/web-search-cache",
        ".cargo/registry",
        ".cargo/git",
        ".agents/skills",
        ".cargo/bin",
        ".rustup",
        ".local/share/claude",
        ".ssh",
        ".aws",
        ".kube",
        ".docker",
        ".config/gh",
        ".config/unrelated-secret",
        ".gnupg",
    ] {
        w.directory(path);
    }
    for path in [
        ".claude.json",
        ".pi/web-search.json",
        ".gitconfig",
        ".cargo/credentials.toml",
    ] {
        fs::write(w.home.path().join(path), "{}").unwrap();
    }
    let output = w.dry_run().assert().success().get_output().stdout.clone();
    let output = String::from_utf8(output).unwrap();
    for path in [
        ".claude",
        ".codex",
        ".gemini",
        ".pi/agent",
        ".pi/context-mode",
        ".pi/web-search-cache",
        ".pi/web-search.json",
        ".cargo/registry",
        ".cargo/git",
    ] {
        assert!(
            output.contains(&format!("-pBindPaths={}/{}", w.home.path().display(), path)),
            "{path}: {output}"
        );
    }
    for path in [
        ".agents",
        ".cargo/bin",
        ".rustup",
        ".local/share/claude",
        ".config/gh",
    ] {
        assert!(output.contains(&format!(
            "-pBindReadOnlyPaths={}/{}",
            w.home.path().display(),
            path
        )));
    }
    for path in [
        ".ssh",
        ".aws",
        ".kube",
        ".docker",
        ".config/unrelated-secret",
        ".gnupg",
        ".claude.json",
        ".gitconfig",
        ".cargo/credentials.toml",
    ] {
        assert!(
            !output.contains(&format!("{}/{}", w.home.path().display(), path)),
            "exposed {path}"
        );
    }
    assert!(!output.contains("PrivateNetwork="));
    assert!(output.contains("-pInaccessiblePaths=-/run/docker.sock"));
    assert!(output.contains("-pInaccessiblePaths=-/run/podman"));
}

#[test]
fn absent_state_is_not_created_or_mounted() {
    let w = Workspace::new();
    w.dry_run()
        .assert()
        .success()
        .stdout(predicate::str::contains("/.codex").not());
    assert!(!w.home.path().join(".codex").exists());
}

#[test]
fn clears_environment_and_keeps_terminal_and_path() {
    let w = Workspace::new();
    w.dry_run()
        .env("AWS_SECRET_ACCESS_KEY", "do-not-forward")
        .env("ANTHROPIC_API_KEY", "also-secret")
        .env("GITHUB_TOKEN", "github-env-secret")
        .env("SSH_AUTH_SOCK", "/host/ssh.sock")
        .env("TERM", "xterm-256color")
        .env("LANG", "C.UTF-8")
        .assert()
        .success()
        .stdout(predicate::str::contains("/usr/bin/env -i --"))
        .stdout(predicate::str::contains("--expand-environment=no"))
        .stdout(predicate::str::contains("--setenv").not())
        .stdout(predicate::str::contains("do-not-forward").not())
        .stdout(predicate::str::contains("also-secret").not())
        .stdout(predicate::str::contains("github-env-secret").not())
        .stdout(predicate::str::contains("SSH_AUTH_SOCK").not())
        .stdout(predicate::str::contains("TERM=xterm-256color"))
        .stdout(predicate::str::contains("LANG=C.UTF-8"))
        .stdout(predicate::str::contains("PATH=/usr/bin"));
}

#[test]
fn explicit_environment_capture_is_an_opt_in() {
    let w = Workspace::new();
    w.command()
        .env("ANTHROPIC_API_KEY", "explicit-secret-$literal")
        .env("DBUS_SESSION_BUS_ADDRESS", "do-not-forward")
        .env("BASH_FUNC_example%%", "do-not-forward")
        .args([
            "--capture-env",
            "true",
            "--profile",
            "coding-agent",
            "--dry-run",
            "--",
            "echo",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "ANTHROPIC_API_KEY=explicit-secret-$literal",
        ))
        .stdout(predicate::str::contains("do-not-forward").not())
        .stdout(predicate::str::contains("/usr/bin/env -i --"));
}

#[test]
fn explicit_capture_path_false_uses_system_path() {
    let w = Workspace::new();
    w.command()
        .env("PATH", "/nonexistent")
        .args([
            "--profile",
            "coding-agent",
            "--capture-path",
            "false",
            "--dry-run",
            "--",
            "echo",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "PATH=/usr/local/bin:/usr/bin:/bin",
        ))
        .stdout(predicate::str::contains("/nonexistent").not());
}

#[test]
fn explicit_flags_override_agent_defaults_in_either_order() {
    let w = Workspace::new();
    for args in [
        vec![
            "--profile",
            "coding-agent",
            "--protect-system",
            "none",
            "--protect-home",
            "read-only",
            "--private-network",
            "true",
            "-m",
            "8G",
        ],
        vec![
            "--protect-system",
            "none",
            "--protect-home",
            "read-only",
            "--private-network",
            "true",
            "-m",
            "8G",
            "--profile",
            "coding-agent",
        ],
    ] {
        w.command()
            .args(args)
            .args(["--dry-run", "--", "echo"])
            .assert()
            .success()
            .stdout(predicate::str::contains("ProtectSystem=").not())
            .stdout(predicate::str::contains("ProtectHome=read-only"))
            .stdout(predicate::str::contains("PrivateNetwork=yes"))
            .stdout(predicate::str::contains("MemoryMax=8G"));
    }
}

#[test]
fn rejects_broad_and_sensitive_workspaces_including_symlink_aliases() {
    let w = Workspace::new();
    w.directory(".ssh");
    symlink(w.home.path().join(".ssh"), w.home.path().join("alias")).unwrap();
    for cwd in [
        w.home.path().to_path_buf(),
        w.home.path().parent().unwrap().to_path_buf(),
        w.home.path().join(".ssh"),
        w.home.path().join("alias"),
        Path::new("/").to_path_buf(),
    ] {
        w.dry_run()
            .current_dir(cwd)
            .assert()
            .failure()
            .stderr(predicate::str::contains("project directory"));
    }
}

#[test]
fn automatic_symlink_mounts_require_explicit_review() {
    let w = Workspace::new();
    w.directory(".ssh");
    symlink(w.home.path().join(".ssh"), w.home.path().join(".codex")).unwrap();
    w.dry_run()
        .assert()
        .failure()
        .stderr(predicate::str::contains("automatic coding-agent mount"));
    w.command()
        .args(["--profile", "coding-agent", "--rw"])
        .arg(w.home.path().join(".codex"))
        .args(["--dry-run", "--", "echo"])
        .assert()
        .success();
}

#[test]
fn discovers_npm_cli_and_runtime_without_exposing_whole_prefix() {
    let w = Workspace::new();
    w.directory("prefix/bin");
    w.directory("prefix/lib/node_modules/@example/agent");
    let cli = w
        .home
        .path()
        .join("prefix/lib/node_modules/@example/agent/cli.js");
    let node = w.home.path().join("prefix/bin/node");
    for path in [&cli, &node] {
        fs::write(path, "#!/bin/sh\nexit 0\n").unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    }
    let pi = w.home.path().join("prefix/bin/pi");
    symlink(&cli, &pi).unwrap();
    let output = w
        .dry_run()
        .env("PATH", w.home.path().join("prefix/bin"))
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let output = String::from_utf8(output).unwrap();
    for path in [
        &w.home.path().join("prefix/bin"),
        &cli,
        &node,
        &w.home.path().join("prefix/lib/node_modules"),
    ] {
        assert!(output.contains(&format!("-pBindReadOnlyPaths={}", path.display())));
    }
    assert!(!output.contains(&format!(
        "-pBindReadOnlyPaths={} ",
        w.home.path().join("prefix").display()
    )));
}

#[test]
fn refuses_workspace_inside_a_relocated_credential_directory() {
    let w = Workspace::new();
    let credentials = common::create_temp_dir();
    symlink(credentials.path(), w.home.path().join(".ssh")).unwrap();
    w.dry_run()
        .current_dir(credentials.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("project directory"));
}

#[test]
fn claude_uses_directory_state_without_copying_legacy_configuration() {
    let w = Workspace::new();
    w.directory(".claude");
    fs::write(w.home.path().join(".claude.json"), "legacy config").unwrap();
    w.dry_run()
        .assert()
        .success()
        .stdout(predicate::str::contains(format!(
            "CLAUDE_CONFIG_DIR={}/.claude",
            w.home.path().display()
        )))
        .stdout(predicate::str::contains("/.claude.json").not());
    assert_eq!(
        fs::read_to_string(w.home.path().join(".claude.json")).unwrap(),
        "legacy config"
    );
    assert!(!w.home.path().join(".claude/.claude.json").exists());
}

#[test]
fn whitespace_in_mount_paths_is_quoted_for_systemd() {
    let w = Workspace::new();
    w.directory("project with % space");
    let project = w.home.path().join("project with % space");
    w.dry_run()
        .current_dir(&project)
        .assert()
        .success()
        .stdout(predicate::str::contains(format!(
            "-pBindPaths=\"{}\"",
            project.display()
        )))
        .stdout(predicate::str::contains("%%").not());
}

#[test]
fn missing_agent_command_cannot_print_the_token_environment() {
    let w = Workspace::new();
    w.command()
        .env("GH_TOKEN", "must-not-print")
        .args(["--profile", "coding-agent"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("requires a command"))
        .stdout(predicate::str::contains("must-not-print").not())
        .stderr(predicate::str::contains("must-not-print").not());
}

#[test]
fn explicit_gh_token_is_referenced_by_name_never_rendered() {
    let w = Workspace::new();
    for capture in ["false", "true"] {
        w.command()
            .env("GH_TOKEN", "secret-$literal with spaces")
            .args([
                "--profile",
                "coding-agent",
                "--capture-env",
                capture,
                "--dry-run",
                "--",
                "gh",
                "api",
                "user",
            ])
            .assert()
            .success()
            .stdout(predicate::str::contains("--setenv=GH_TOKEN "))
            .stdout(predicate::str::contains("GH_TOKEN=${GH_TOKEN}"))
            .stdout(predicate::str::contains("secret-$literal").not());
    }
    w.dry_run()
        .env("GH_TOKEN", "")
        .assert()
        .success()
        .stdout(predicate::str::contains("--setenv=GH_TOKEN").not());
}

#[test]
fn discovers_gh_on_path_without_mounting_sibling_tools() {
    let w = Workspace::new();
    w.directory("custom/bin");
    // Even a native binary beside an npm prefix should be mounted individually.
    w.directory("custom/lib/node_modules");
    let gh = w.home.path().join("custom/bin/gh");
    fs::write(&gh, "#!/bin/sh\nexit 0\n").unwrap();
    fs::set_permissions(&gh, fs::Permissions::from_mode(0o755)).unwrap();
    fs::write(w.home.path().join("custom/bin/private-tool"), "private").unwrap();
    w.dry_run()
        .env("PATH", w.home.path().join("custom/bin"))
        .assert()
        .success()
        .stdout(predicate::str::contains(format!(
            "-pBindReadOnlyPaths={}",
            gh.display()
        )))
        .stdout(
            predicate::str::contains(format!(
                "-pBindReadOnlyPaths={} ",
                w.home.path().join("custom/bin").display()
            ))
            .not(),
        )
        .stdout(predicate::str::contains("private-tool").not());
}

#[test]
fn gh_config_location_follows_gh_then_xdg_then_home() {
    let w = Workspace::new();
    for dir in [".config/gh", "xdg/gh", "custom-$literal"] {
        w.directory(dir);
    }
    for (gh, xdg, expected) in [
        (None, None, ".config/gh"),
        (None, Some("xdg"), "xdg/gh"),
        (Some("custom-$literal"), Some("xdg"), "custom-$literal"),
    ] {
        let mut cmd = w.dry_run();
        if let Some(path) = gh {
            cmd.env("GH_CONFIG_DIR", w.home.path().join(path));
        }
        if let Some(path) = xdg {
            cmd.env("XDG_CONFIG_HOME", w.home.path().join(path));
        }
        let expected = w.home.path().join(expected);
        cmd.assert()
            .success()
            .stdout(predicate::str::contains(format!(
                "-pBindReadOnlyPaths={}",
                expected.display()
            )))
            .stdout(predicate::str::contains(format!(
                "GH_CONFIG_DIR={}",
                expected.display()
            )))
            .stdout(predicate::str::contains(format!("-pBindPaths={}", expected.display())).not())
            .stdout(predicate::str::contains("XDG_CONFIG_HOME=").not());
    }
}

#[test]
fn absent_gh_config_is_not_mounted_or_created() {
    let w = Workspace::new();
    w.dry_run().assert().success().stdout(
        predicate::str::contains(format!(
            "-pBindReadOnlyPaths={}/.config/gh",
            w.home.path().display()
        ))
        .not(),
    );
    assert!(!w.home.path().join(".config/gh").exists());
}

#[test]
fn gh_config_rejects_broad_paths_and_unreviewed_symlinks() {
    let w = Workspace::new();
    w.directory(".config");
    w.directory(".ssh");
    for path in [
        w.home.path().to_path_buf(),
        w.home.path().join(".config"),
        w.home.path().join(".ssh"),
        Path::new("relative").to_path_buf(),
        Path::new("/run/user").to_path_buf(),
    ] {
        w.dry_run()
            .env("GH_CONFIG_DIR", path)
            .assert()
            .failure()
            .stderr(predicate::str::contains(
                "dedicated absolute GitHub CLI configuration directory",
            ));
    }
    symlink(w.home.path().join(".ssh"), w.home.path().join(".config/gh")).unwrap();
    w.dry_run()
        .assert()
        .failure()
        .stderr(predicate::str::contains("automatic coding-agent mount"));
}

#[test]
fn other_profiles_keep_legacy_environment_and_system_protection() {
    let w = Workspace::new();
    w.command()
        .args(["--profile", "python", "--dry-run", "--", "python3"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--setenv=PATH="))
        .stdout(predicate::str::contains("/usr/bin/env -i").not())
        .stdout(predicate::str::contains("ProtectSystem=").not());
}
