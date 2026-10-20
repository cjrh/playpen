# playpen - Agent Developer Guide

This document provides guidance to coding agents working on the `playpen` repository.

## Project Overview

**playpen** is a Rust CLI tool that wraps `systemd-run` to launch programs with memory limits, CPU limits, and filesystem sandboxing. It is designed to prevent runaway processes from consuming system resources by using systemd's cgroup features.

The tool is Linux-only and requires systemd.

## Build and Development Commands

```bash
# Build
cargo build
cargo build --release

# Run (note the -- separator)
cargo run -- [playpen args] -- [command]
# Example: cargo run -- -m 50M -- python3

# Test
cargo test

# Check and format
cargo check
cargo clippy
cargo fmt

# Coverage (requires cargo-llvm-cov)
cargo coverage          # generates lcov.info + terminal summary
cargo coverage-html     # opens HTML report in browser
```

## Architecture

- **Single binary**: All code is in `src/main.rs`
- **CLI parsing**: Uses `clap` with derive macros
- **Core functionality**: Builds and executes `systemd-run` commands with appropriate flags, then replaces the current process via `execvp`

### Key Dependencies

| Crate | Purpose |
|-------|---------|
| `clap` | CLI argument parsing with derive feature |
| `nix` | Unix system calls (`execvp`) |
| `anyhow` | Error handling |
| `atty` | TTY detection for `--pty` flag |
| `shellexpand` | Shell-style environment variable expansion for profile paths |

### Core Logic Flow

1. Parse CLI arguments using `clap` (`Run` struct)
2. Build a `Vec<String>` containing `systemd-run` arguments with base flags (`--user --same-dir --wait --pipe`)
3. Resolve resource limits and sandboxing into a `Config`; coding-agent also resolves its sanitized command environment here
4. Add legacy environment capture for other profiles and TTY detection (`--pty`)
5. Render `Config` to `systemd-run` properties and prepend its command environment, if any
6. Execute via `execvp` to replace the current process with `systemd-run`

### The `Config` Resolver

All sandbox settings flow through `Config`, a plain struct holding the resolved values. `clap` parses every flag (no second hand-rolled parser); `Config::resolve` then merges them with this fixed precedence, lowest to highest:

1. Built-in defaults (the four namespace protections on, nothing else set)
2. `--profile` baseline
3. The `--current-dir-only` preset
4. Explicit per-setting flags

**Command-line order is irrelevant** — an explicit flag always beats the profile. This is why override flags are typed `Option<T>`: `None` means "not given, keep the profile/default value". Path flags (`--rw`/`--ro`/`--inaccessible`) accumulate rather than override.

```bash
# Explicit flag wins over the profile, whichever side of --profile it sits on
playpen --profile cargo -m 4G -- cargo build   # MemoryMax=4G
playpen -m 4G --profile cargo -- cargo build   # MemoryMax=4G
```

`Config::to_systemd_args` is the single point that renders the resolved struct into `-p...` arguments; nothing downstream re-reads the CLI.

### Profile System

Profiles bundle resource limits and filesystem access for common tools. There are 9 built-in profiles defined as a `const PROFILES: &[Profile]`:

| Profile | Memory | CPU | Swap | ProtectHome | Extra paths |
|---------|--------|-----|------|-------------|-------------|
| `cargo` | 2G | 300% | disabled | tmpfs | rw: `~/.cargo`, ro: `~/.rustup` |
| `npm` | 1G | 200% | disabled | tmpfs | rw: npm/yarn/pnpm caches, ro: fnm |
| `pytest` | 512M | 200% | disabled | tmpfs | ro: `~/.local/lib` |
| `python` | 512M | 100% | disabled | tmpfs | ro: `~/.local/lib` |
| `uv` | 256M | 200% | disabled | tmpfs | rw: `~/.cache/uv`, `~/.local/share/uv` |
| `go` | 512M | 300% | **enabled** | tmpfs | rw: `~/go`, `~/.cache/go-build` |
| `make` | 2G | 300% | disabled | tmpfs | — |
| `coding-agent` | 4G | 200% | disabled | tmpfs | rw: detected agent state + Cargo caches; ro: `~/.agents`, gh config, toolchains + PATH-detected executables; no SSH/git config |
| `shell` | 4G | unlimited | disabled | **read-only** | rw: `~/.local/share`, `~/.cache`, `~/.local/bin` |

Profile paths containing `$HOME` and `$UID` are expanded at runtime via `shellexpand`. Non-existent paths are silently skipped (not added as bind mounts).

Activating a profile copies its limits, `protect_home`, `protect_system`, and
`private_network` into `Config`, with `bind_cwd` forced to `true`. The four
namespace protections remain on by default. `Config::resolve` returns `Result`
because coding-agent discovery and workspace validation can fail.

Only `coding-agent` sets `Profile::agent_access`. It uses `ProtectSystem=strict`,
rejects broad/sensitive working directories and unreviewed automatic directory
symlinks, masks rootful container sockets, discovers selected tools on PATH, and
builds a sanitized `env -i` command prefix in `Config`. `--capture-env true` opts
back into shell environment capture; other profiles retain legacy behavior.
Version-manager PATH aliases are canonicalized; standard npm `bin` directories
are mounted intact so CLI symlinks keep working. Do not bind an npm launcher
symlink as a single file: that flattens it and breaks relative imports.

The coding-agent profile intentionally shares every detected supported agent's
state, including credentials and writable settings/extensions. It is not an
agent-to-agent credential boundary or a hostile-code/network sandbox. All of
`~/.agents` is read-only. Claude uses `CLAUDE_CONFIG_DIR=$HOME/.claude`, with
one-time host setup documented in README; legacy `~/.claude.json` is neither
mounted nor copied because file binds cannot support atomic replacement.
Systemd 254+ is required for this profile's `--expand-environment=no`.

`gh` is discovered on PATH, never through a hard-coded personal location.
`github_config_dir` follows GH_CONFIG_DIR > XDG_CONFIG_HOME/gh > ~/.config/gh;
only that directory is exposed read-only. File-backed credentials permit remote
writes within their token permissions; the host keyring/user bus remain hidden.
An explicitly supplied nonempty GH_TOKEN is forwarded by name using
`--setenv=GH_TOKEN` and GNU `env -S '-- GH_TOKEN=${GH_TOKEN}'`. Never put its value
in command arguments/dry-run output, even with full environment capture. Playpen
must not query the host keyring itself. Other shell tokens still require opt-in.

### Resource Limits

- **Memory**: `-pMemoryMax=<value>`. If set without a profile and without explicit `--memory-swap-max`, defaults to `-pMemorySwapMax=0` (no swap).
- **CPU**: `-pCPUQuota=<value>` with fixed `-pCPUQuotaPeriodSec=100ms`.
- **Swap**: Controlled via `--memory-swap-max`. Go profile sets `None` (swap allowed); all others set `"0"`.

## Testing

### Integration Tests

All tests are integration tests in the `tests/` directory:

- **`tests/test_home_access.rs`** — 9 tests for path restriction behavior (`--protect-home`, `--current-dir-only`, `--rw`, `--ro`, `--inaccessible`)
- **`tests/test_npm_project.rs`** — 5 tests for npm-specific sandboxing (requires npm/node on the system; skips if unavailable)
- **`tests/test_profiles.rs`** — 13 tests for profile dry-run output, override precedence (explicit flags beat the profile, order-independent), unknown profile errors, and path accumulation
- **`tests/test_coding_agent.rs`** — isolated dry-run tests for agent discovery, credential exclusions, environment policy, symlink/workspace guards, and overrides
- **`tests/test_coding_agent_live.rs`** — ignored live test using synthetic agent state: filesystem/environment/socket boundaries, enforced resource limits, npm symlinks, Claude atomic writes, and gh executable/config/token access
- **`tests/test_network.rs`** — 23 tests for network-control flags (`--private-network`, `--ip-allow`/`--ip-deny`, `--socket-bind-allow`/`--socket-bind-deny`); all use `--dry-run` and run everywhere
- **`tests/test_disk_io.rs`** — 8 tests for disk I/O bandwidth flags (`--disk-limit`/`-d`, `--disk-read`, `--disk-write`), including direction override precedence; all use `--dry-run` and run everywhere

Tests that invoke `systemd-run` for real (`test_home_access.rs`, `test_npm_project.rs`) are marked `#[ignore]` because GitHub-hosted CI runners cannot set up the mount namespaces they need (systemd exit code 218). Run them locally with `cargo test -- --include-ignored`. The `test_profiles.rs` tests use `--dry-run` and run everywhere.
- **`tests/common/mod.rs`** — Shared test utilities and binary path resolution. Carries `#![allow(dead_code)]`: it is compiled into every test binary, so a helper used by only one of them looks dead to the others.

CLI profile names use hyphens (e.g. `coding-agent`); the matching Rust test function names substitute underscores (`test_coding_agent_profile_dry_run`) because Rust identifiers cannot contain hyphens. This mismatch is expected, not a bug — keep it in mind when renaming a profile.

### Code Coverage

Coverage uses `cargo-llvm-cov` with aliases defined in `.cargo/config.toml`:

```bash
cargo coverage          # lcov.info + terminal summary
cargo coverage-html     # HTML report in target/llvm-cov/html
```

Unit and integration test coverage is automatically merged. The CI workflow at `.github/workflows/coverage.yml` generates `lcov.info` on push/PR.

## Key Features to Understand

- **Profiles**: Predefined bundles of limits + filesystem access. Activated with `--profile <name>`. Always overrideable.
- **Override precedence**: defaults < profile < `--current-dir-only` < explicit flags. Command-line order does not matter.
- **Environment variable handling**: Supports capturing full environment (`--capture-env`) or just PATH (`--capture-path`, default). Filters out `DBUS_SESSION_BUS_ADDRESS` and exported Bash functions.
- **TTY detection**: Automatically adds `--pty` when both stdin and stdout are TTYs.
- **Systemd integration**: All process management is delegated to `systemd-run`. playpen itself is just a command builder.
- **Path expansion**: Profile paths use `shellexpand::env()` for `$HOME`, `$UID`, etc.
- **Non-existent paths**: Silently skipped to avoid `systemd-run` failures on paths the user doesn't have.

## Project Organization

- **`src/main.rs`** — Single binary source
- **`tests/`** — Integration tests (`test_home_access.rs`, `test_npm_project.rs`, `test_profiles.rs`, `test_coding_agent.rs`, `test_coding_agent_live.rs`, `test_network.rs`, `test_disk_io.rs`)
- **`.cargo/config.toml`** — Cargo aliases (`coverage`, `coverage-html`)
- **`plans/`** — Design documents and implementation plans (e.g., `PROFILES_PLAN.md`)

The `## CLI Docs` block in `README.md` is a hand-maintained snapshot of `playpen -h`. Whenever a CLI flag is added, removed, or reworded, regenerate it with `cargo run --quiet -- -h`.

When a commit removes or renames a key type or function named in this file, update this file in the same commit.

## Platform Constraints

- **Linux only**: Depends on `systemd-run` and `execvp`.
- **Environment escaping**: Legacy capture uses literal double quotes (`--setenv=KEY="VAL"`). The coding-agent profile instead passes separate `KEY=value` arguments through `env -i`, with systemd environment expansion disabled.
- **Execvp**: The process is replaced; `systemd-run` becomes the running process. This means normal `return`/`exit` from `main()` only happens on `--dry-run` or errors before `execvp`.
