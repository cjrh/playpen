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

- **Single binary**: All code is in `src/main.rs` (~700+ lines)
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
3. Add environment variable handling (PATH or full env based on `--capture-env`/`--capture-path`)
4. Add TTY detection for interactive mode (`--pty`)
5. Build resource limits and sandboxing properties via the `SystemdProps` stateful builder
6. Execute via `execvp` to replace the current process with `systemd-run`

### The `SystemdProps` Builder

The heart of the command construction is `SystemdProps`, a stateful builder that implements **rightmost-wins** semantics:

- Arguments are processed in command-line order (left to right)
- Each flag modifies the running state in `SystemdProps`
- The **last** occurrence of any setting wins
- This applies to: `--profile`, `-m`, `-c`, `--memory-swap-max`, `--protect-home`, `--current-dir-only`, etc.

For example:
```bash
# Explicit flag AFTER profile: explicit wins
playpen --profile cargo -m 4G -- cargo build   # Result: MemoryMax=4G

# Profile AFTER explicit flag: profile wins
playpen -m 4G --profile cargo -- cargo build    # Result: MemoryMax=2G
```

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
| `coding_agent` | 4G | 200% | disabled | tmpfs | ro: `~/.gitconfig`, `~/.ssh` |
| `shell` | 4G | unlimited | disabled | **read-only** | rw: `~/.local/share`, `~/.cache`, `~/.local/bin` |

Profile paths containing `$HOME` and `$UID` are expanded at runtime via `shellexpand`. Non-existent paths are silently skipped (not added as bind mounts).

When a profile's `protect_home` is not `"no"`, the lockdown base is applied: `PrivateTmp=yes`, `PrivateDevices=yes`, `ProtectKernelTunables=yes`, `ProtectControlGroups=yes`, plus `BindPaths=CWD`.

### Argument Processing

The `process_ordered_args()` function walks `std::env::args()` directly (bypassing `clap` for ordering) to implement rightmost-wins. It handles both `--flag=value` and `--flag value` forms for:

- `--profile`, `-m`/`--memory-limit`, `-c`/`--cpu-limit`, `--memory-swap-max`
- `--protect-home`, `--protect-system`
- `--private-tmp`, `--private-devices`, `--protect-kernel-tunables`, `--protect-control-groups`
- `--current-dir-only`

After ordered processing, `apply_defaults()` sets default protections for any boolean flags not explicitly touched by the user.

### Resource Limits

- **Memory**: `-pMemoryMax=<value>`. If set without a profile and without explicit `--memory-swap-max`, defaults to `-pMemorySwapMax=0` (no swap).
- **CPU**: `-pCPUQuota=<value>` with fixed `-pCPUQuotaPeriodSec=100ms`.
- **Swap**: Controlled via `--memory-swap-max`. Go profile sets `None` (swap allowed); all others set `"0"`.

## Testing

### Integration Tests

All tests are integration tests in the `tests/` directory:

- **`tests/test_home_access.rs`** — 9 tests for path restriction behavior (`--protect-home`, `--current-dir-only`, `--rw`, `--ro`, `--inaccessible`)
- **`tests/test_npm_project.rs`** — 5 tests for npm-specific sandboxing (requires npm/node on the system; skips if unavailable)
- **`tests/test_profiles.rs`** — 14 tests for profile dry-run output, rightmost-wins precedence, override behavior, unknown profile errors, and path accumulation
- **`tests/common/mod.rs`** — Shared test utilities and binary path resolution

### Code Coverage

Coverage uses `cargo-llvm-cov` with aliases defined in `.cargo/config.toml`:

```bash
cargo coverage          # lcov.info + terminal summary
cargo coverage-html     # HTML report in target/llvm-cov/html
```

Unit and integration test coverage is automatically merged. The CI workflow at `.github/workflows/coverage.yml` generates `lcov.info` on push/PR.

## Key Features to Understand

- **Profiles**: Predefined bundles of limits + filesystem access. Activated with `--profile <name>`. Always overrideable.
- **Rightmost-wins**: Command-line order determines final values. No special-casing of profile vs. explicit flags.
- **Environment variable handling**: Supports capturing full environment (`--capture-env`) or just PATH (`--capture-path`, default). Filters out `DBUS_SESSION_BUS_ADDRESS` and exported Bash functions.
- **TTY detection**: Automatically adds `--pty` when both stdin and stdout are TTYs.
- **Systemd integration**: All process management is delegated to `systemd-run`. playpen itself is just a command builder.
- **Path expansion**: Profile paths use `shellexpand::env()` for `$HOME`, `$UID`, etc.
- **Non-existent paths**: Silently skipped to avoid `systemd-run` failures on paths the user doesn't have.

## Project Organization

- **`src/main.rs`** — Single binary source (~700+ lines)
- **`tests/`** — Integration tests (`test_home_access.rs`, `test_npm_project.rs`, `test_profiles.rs`)
- **`.cargo/config.toml`** — Cargo aliases (`coverage`, `coverage-html`)
- **`plans/`** — Design documents and implementation plans for previously shipped releases (e.g., `PROFILES_PLAN.md`)

## Platform Constraints

- **Linux only**: Depends on `systemd-run` and `execvp`.
- **Environment escaping**: Environment values are wrapped in literal double quotes (`--setenv=KEY="VAL"`).
- **Execvp**: The process is replaced; `systemd-run` becomes the running process. This means normal `return`/`exit` from `main()` only happens on `--dry-run` or errors before `execvp`.
