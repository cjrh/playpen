![playpen](playpen_transparent2.png)

# playpen
Program launcher with memory, cpu, and path limits

## Overview

I wanted a way to run a program with memory and CPU limits. It turns out
that `systemd-run` does exactly this, _and it doesn't require root_, but
the necessary parameters are many and confusing. So playpen wraps all that
up in a simple CLI which is set up for the typical use cases I have.

## CLI Docs

```
$ playpen -h
Usage: playpen [OPTIONS] [COMMAND_AND_ARGS]...


Arguments:
  [COMMAND_AND_ARGS]...

Options:
  -m, --memory-limit <MEMORY_LIMIT>

  -c, --cpu-limit <CPU_LIMIT>

  -q, --quiet

      --capture-env <CAPTURE_ENV>
          [default: false] [possible values: true, false]
      --capture-path <CAPTURE_PATH>
          [default: true] [possible values: true, false]
      --profile <NAME>
          Use a predefined resource and filesystem profile
      --memory-swap-max <VALUE>
          Set MemorySwapMax limit (e.g., 0, 1G)
      --dry-run
          Print the resolved systemd-run command without executing
      --rw <RW_PATHS>
          Add read-write path access (can be repeated)
      --ro <RO_PATHS>
          Add read-only path access (can be repeated)
      --inaccessible <INACCESSIBLE>
          Make path completely inaccessible (can be repeated)
      --private-tmp <PRIVATE_TMP>
          Use private /tmp [default: true] [possible values: true, false]
      --private-devices <PRIVATE_DEVICES>
          Use private /dev [default: true] [possible values: true, false]
      --protect-kernel-tunables <PROTECT_KERNEL_TUNABLES>
          Protect kernel tunables [default: true] [possible values: true, false]
      --protect-control-groups <PROTECT_CONTROL_GROUPS>
          Protect control groups [default: true] [possible values: true, false]
      --protect-home <PROTECT_HOME>
          Protect home directories: none/yes/read-only/tmpfs
      --protect-system <PROTECT_SYSTEM>
          Protect system directories: none/yes/full/strict
      --private-network <PRIVATE_NETWORK>
          Use a private network namespace, no external network (default: off) [possible values: true, false]
      --ip-allow <IP_ALLOW>
          Allow IP/CIDR for network traffic (can be repeated)
      --ip-deny <IP_DENY>
          Deny IP/CIDR for network traffic (can be repeated)
      --socket-bind-allow <SOCKET_BIND_ALLOW>
          Allow bind() rule for listening sockets (can be repeated)
      --socket-bind-deny <SOCKET_BIND_DENY>
          Deny bind() rule for listening sockets (can be repeated)
      --current-dir-only
          Restrictive preset: only current directory accessible
  -h, --help
          Print help
  -V, --version
          Print version
```

## Demo

This interactive python session is killed by the OOM killer because
it exceeds the 50 MB memory limit given:

```
$ playpen -m 50M python3
Running as unit: run-u544.service; invocation ID: 0bf06fd9a32b4e619993845b58066edd
Press ^] three times within 1s to disconnect TTY.

Python 3.12.3 (main, Nov  6 2024, 18:32:19) [GCC 13.2.0] on linux
Type "help", "copyright", "credits" or "license" for more information.
>>> x = [0] * 2**30

Finished with result: oom-kill
Main processes terminated with: code=killed/status=KILL
Service runtime: 11.423s
CPU time consumed: 103ms
Memory peak: 50.0M
Memory swap peak: 0B
```

This event will also show up in the system logs:

```journalctl
$ sudo journalctl --since "1 minute ago"
Dec 14 17:13:12 kernel: python3 invoked oom-killer: gfp_mask=0xcc0(GFP_KERNEL), order=0, oom_score_adj=200
Dec 14 17:13:12 kernel: CPU: 12 PID: 483880 Comm: python3 Tainted: P           O       6.8.0-50-generic #51-Ubuntu
Dec 14 17:13:12 kernel: Hardware name: XXXXXXXXXXXXXXXXXXXXXXXXXXXX, XXXXXXXXXXXXXXXXXXXX XXXXXXXXXX
...
Dec 14 17:13:12 kernel: memory: usage 51200kB, limit 51200kB, failcnt 37
Dec 14 17:13:12 kernel: swap: usage 0kB, limit 0kB, failcnt 0
Dec 14 17:13:12 kernel: Memory cgroup stats for /user.slice/user-1000.slice/user@1000.service/app.slice/run-u...
...
Dec 14 17:13:12 kernel: Tasks state (memory values in pages):
Dec 14 17:13:12 kernel: [  pid  ]   uid  tgid total_vm      rss rss_anon rss_file rss_shmem pgtables_bytes sw...
Dec 14 17:13:12 kernel: [ 483880]  1000 483880    73357    14530    12706     1824         0   180224        ...
Dec 14 17:13:12 kernel: oom-kill:constraint=CONSTRAINT_MEMCG,nodemask=(null),cpuset=user.slice,mems_allowed=0...
Dec 14 17:13:12 kernel: Memory cgroup out of memory: Killed process 483880 (python3) total-vm:293428kB, anon-...
Dec 14 17:13:12 systemd[2765]: run-u820.service: A process of this unit has been killed by the OOM killer.
Dec 14 17:13:12 systemd[2765]: run-u820.service: Main process exited, code=killed, status=9/KILL
Dec 14 17:13:12 systemd[2765]: run-u820.service: Failed with result 'oom-kill'.
Dec 14 17:13:12 systemd[2765]: Failed to reset TTY ownership/access mode of /dev/pts/2 to 0:5, ignoring: Oper...
Dec 14 17:13:12 systemd[1]: user@1000.service: A process of this unit has been killed by the OOM killer.

```

In data science workloads, it is common to run out of memory and have
the system start swapping, which can make the system unresponsive. With
playpen, you can set a memory limit to bound a process, which will be
OOM-killed if it exceeds the limit. So you don't have to restart the
machine!

A CPU limit can also be set:

```
$ playpen -m 50M -c 100% python3
```

For CPU, "100%" means 1 core, "200%" means 2 cores, and so on. This can
be used to force a process to run on fewer than all available cores
on a machine if the process does not have a simple, built-in way to limit 
itself.

## Capturing `$PATH` and the environment

By default, `playpen` captures the `$PATH`, sending this through to
the underlying `systemd-run` command. However, the environment is not
captured by default, for safety reasons. This can result in errors if
your task invocation relies on environment variables in your calling
environment. Note that `capture-env` supercedes `capture-path` if enabled,
so if `capture-env` is on, `capture-path` is ignored.

This also applies to env vars supplied on the path:

```bash
$ ABC=123 playpen --capture-env=on -- bash -c 'env'
<snip>
ABC=123
<snip>
```

But if the option is off, the `ABC` variable is not passed through.
My suggestion is that you should rather use a `.env` file and have
your application read that instead of relying on the live environment.
This is much safer because you won't get inadvertent leakage of
information unintended for the child process.

There are some interactive use-cases where passing the entire environment
is very convenient, and where playpen is really just used as a proxy
for an interactive command. Shells like python are an example of this,
but also build tools like `npm run dev`, where a dev server runs to 
serve a web application in a development environment. An invocation for
this looks like this:

```bash
$ playpen -m 4G --capture-env=on -- npm run dev
```

Occasionally this dev servers get memory leaks, making playpen more
useful ;).

## Profiles

`playpen` includes predefined profiles that bundle resource limits and filesystem access for common tools. Profiles set sensible defaults for memory, CPU, and filesystem access so you don't have to figure out what paths each tool needs.

### Available Profiles

| Profile | Purpose | Memory | CPU | Swap |
|---------|---------|--------|-----|------|
| `cargo` | Rust/Cargo builds and tests | 2G | 300% | disabled |
| `npm` | Node.js npm/yarn/pnpm build and test | 1G | 200% | disabled |
| `pytest` | Python pytest | 512M | 200% | disabled |
| `python` | General Python scripts | 512M | 100% | disabled |
| `uv` | Python uv dependency management | 256M | 200% | disabled |
| `go` | Go builds and tests | 512M | 300% | **enabled** |
| `make` | C/C++ make/cmake builds | 2G | 300% | disabled |
| `coding-agent` | AI coding agents (claude, codex, gemini, pi, etc.) | 4G | 200% | disabled |
| `shell` | Interactive shell/terminal session | 4G | unlimited | disabled |

### Basic Usage

Use `--profile <name>` to activate a profile:

```bash
# Build a Rust project
$ playpen --profile cargo -- cargo build

# Run npm install
$ playpen --profile npm -- npm install

# Run Python tests
$ playpen --profile pytest -- pytest

# Start an interactive shell session
$ playpen --profile shell -- bash
```

### Overriding Profile Settings

A profile only supplies a baseline. Any explicit flag you pass overrides the matching profile setting, **regardless of command-line order**. Path flags (`--rw`/`--ro`/`--inaccessible`) accumulate on top of the profile's paths rather than replacing them:

```bash
# Override the memory limit
$ playpen --profile cargo -m 4G -- cargo build

# Override swap behavior
$ playpen --profile cargo --memory-swap-max 1G -- cargo build

# Add extra read-write path
$ playpen --profile cargo --rw $HOME/.ccache -- cargo build
```

### AI Coding Agent Profile

The `coding-agent` profile is designed for AI coding assistants. It hides the entire home directory by default (using `ProtectHome=tmpfs`) to prevent agents from reading secrets in dotfiles, then selectively exposes only what's needed for git operations:

- `~/.gitconfig` (read-only) — git identity for commits
- `~/.ssh` (read-only) — SSH keys for git push/pull
- Current working directory (read-write) — your project code

Agent-specific config directories are **not** included in the profile (for security — different agents shouldn't see each other's configs). You add only what your specific agent needs:

```bash
# Claude Code
$ playpen --profile coding-agent --rw ~/.claude --ro ~/.claude.json -- claude

# OpenAI Codex
$ playpen --profile coding-agent --rw ~/.codex -- codex

# Gemini CLI
$ playpen --profile coding-agent --rw ~/.gemini -- gemini

# pi
$ playpen --profile coding-agent --rw ~/.pi -- pi
```

### Shell Profile

The `shell` profile is designed for interactive terminal sessions. It makes the entire home directory **read-only** (so all your shell config is visible) but grants write access to commonly-used subdirectories:

- `~/.local/share` — app data, shell history
- `~/.cache` — build caches, download caches
- `~/.local/bin` — user-installed tools
- Current working directory — whatever you're working on

No CPU limit is set because a terminal session may run arbitrary workloads.

```bash
$ playpen --profile shell -- bash
```

### Debugging with `--dry-run`

Use `--dry-run` to see the resolved `systemd-run` command without executing it. This is useful for verifying what limits and paths a profile produces:

```bash
$ playpen --profile cargo --dry-run -- cargo build
```

### Symlinked Dotfiles

With `ProtectHome=tmpfs`, home directories are replaced by an empty tmpfs. If a path like `~/.claude.json` is a symlink (e.g., `~/.claude.json -> stowfiles/.claude.json`), bind-mounting only the symlink will create a broken link because the target is also under the hidden `/home` tree. To fix this, also bind-mount the target directory:

```bash
$ playpen --profile coding-agent --ro ~/.claude.json --ro ~/stowfiles -- claude
```

## Path Restrictions

Playpen provides (via `systemd-run`) powerful path access controls to limit what 
directories and files a process can access, helping to isolate and sandbox programs.
This is particularly useful when running untrusted code or preventing programs from
accessing sensitive system files.

### Quick Protection with `--current-dir-only`

The easiest way to enable path restrictions is with the `--current-dir-only` flag,
which applies a restrictive preset that makes only the current working directory
accessible to the program with write access.:

```bash
$ playpen --current-dir-only -m 100M -- npm install
```

This completely blocks access to home directories by mounting them as tmpfs (empty temporary
filesystems) while leaving system directories readable. The program can only access the
current working directory (with read-write access) plus standard system resources needed
for basic operation.

In practice, many tools will require additional access to function properly. For example,
`npm install` typically needs access to the global npm cache in your home directory,
as well as its own executable files. You can grant the necessary access with 
the `--ro` ("read only") and `--rw` ("read write") options, which will override the
tmpfs protection for those paths.

```bash
$ playpen \
    --current-dir-only \
    --rw $(npm config get cache) \
    --ro $(npm config get prefix) \
    -m 2G -- npm install
```

Further additional paths can be added as needed using more `--ro` and `--rw` options.

I'm using the fnm node version manager. It does a few things differently with
paths, so I have to add an additional read-only path for where fnm keeps
the binaries:

```bash
$ playpen \
    --current-dir-only \
    --ro (npm config get prefix) \
    --rw (npm config get cache) \
    --ro /run/user \
    -- npm ci
```

### Fine-Grained Path Control

You can override the default restrictions or create custom access patterns using:

- `--ro <path>`: Grant read-only access to a specific path
- `--rw <path>`: Grant read-write access to a specific path
- `--inaccessible <path>`: Make a path completely inaccessible

These options can be repeated multiple times to configure exactly the access you need:

```bash
# Allow read-only access to /usr/lib and read-write to /tmp
$ playpen --current-dir-only --ro /usr/lib --rw /tmp -m 100M -- my-program

# Make the .env file inaccessible even within current directory
$ playpen --current-dir-only --inaccessible ./.env -- npm test
```

### System Protection Options

Additional system-level protections are available:

- `--private-tmp`: Use a private /tmp directory
- `--private-devices`: Use a private /dev directory
- `--protect-home <mode>`: Protect home directories (none/yes/read-only/tmpfs)
- `--protect-system <mode>`: Protect system directories (none/yes/full/strict)
- `--protect-kernel-tunables`: Protect kernel tunables
- `--protect-control-groups`: Protect control groups

### Example: Securing npm Commands

When running npm commands, you often want to protect your home directory and
system files while allowing access to node_modules and the project directory:

```bash
# Basic protection for npm install
$ playpen --current-dir-only -m 2G -- npm install

# Allow npm to access its global cache
$ playpen --current-dir-only --rw ~/.npm -m 2G -- npm install

# Run tests with extra protection
$ playpen --current-dir-only --private-tmp -m 1G -- npm test
```

## Network Control

Playpen can restrict a sandboxed process's network access. Network control is
**opt-in**: by default a process has full network access, and no profile
changes that. Three independent controls are available.

### `--private-network` — full network isolation

`--private-network true` puts the process in a private network namespace with
only a loopback device. This kills **all** external network access — DNS, HTTP,
SSH, everything — leaving only `127.0.0.1`.

```bash
# No external network at all
$ playpen --private-network true -- ./run-offline-tests

# Loopback still works
$ playpen --private-network true -- ping 127.0.0.1
```

This is the strongest control: namespace isolation rather than packet
filtering. It follows the standard precedence — an explicit `--private-network`
beats the profile regardless of command-line order.

### `--ip-allow` / `--ip-deny` — IP-level filtering

These map to systemd's `IPAddressAllow=`/`IPAddressDeny=` BPF filters and keep
the network otherwise available. They accept **IP addresses, CIDR ranges, and
systemd symbolic names only** (`any`, `localhost`, `link-local`, `multicast`) —
**not** hostnames. Both flags can be repeated.

Per-packet evaluation: a match in the allow-list wins; otherwise a match in the
deny-list denies; otherwise the packet is allowed. To build an allow-list
firewall, deny everything and then add exceptions:

```bash
# Allow only localhost traffic
$ playpen --ip-deny any --ip-allow localhost -- ./my-test

# Allow only a specific host
$ playpen --ip-deny any --ip-allow 192.168.1.100 -- ./my-client

# Deny just one range, leave the rest open
$ playpen --ip-deny 10.0.0.0/8 -- ./my-app
```

> Hostname/domain filtering is intentionally not supported: systemd filters by
> IP, and a launch-time DNS snapshot would be stale and misleading (one CDN IP
> fronts many domains). Use a filtering proxy if you need domain-level egress
> control.

### `--socket-bind-allow` / `--socket-bind-deny` — listen restrictions

These control which addresses/ports a process may `bind()` — i.e. **listen**
on. They do not affect outbound connections. Syntax is
`[address-family:][transport-protocol:][ip-ports]` or `any`. Both flags can be
repeated, and evaluation follows the same allow-wins-then-deny order.

```bash
# Process cannot start any server
$ playpen --socket-bind-deny any -- ./my-test

# Allow listening only on port 8080
$ playpen --socket-bind-allow 8080 --socket-bind-deny any -- ./my-server

# Allow only TCP on port 8080
$ playpen --socket-bind-allow ipv4:tcp:8080 --socket-bind-deny any -- ./my-server
```

### Choosing between them

`PrivateNetwork=yes` (namespace isolation) is stronger than IP filtering (a BPF
filter). With `--private-network true` there are no external interfaces, so IP
filters have nothing external to act on — combining the two is redundant. Use
`--private-network` for full isolation and IP filtering for selective access.
The values you pass to the list flags are forwarded to `systemd-run` verbatim;
an invalid value surfaces as a `systemd-run` error.

## Examples

### Simple example

Wrapping a simple command, like `date`:

```
$ playpen -q date
Sat Dec 14 05:24:10 PM CET 2024
```

Works just like the `date` command. The `-q` means "quiet" and we'll get
to that later.

In the next example, we apply a 1K memory limit to the `date` command:

```
$ playpen -q -m 1K date
```

In this case, there is no output because the process was OOM-killed. It
turns out that `date` needs more than 1K of memory to run:

```
$ playpen -q -m 300K date
Sat Dec 14 05:26:40 PM CET 2024
```

Works with 300K of memory. The `date` command is not very memory-hungry.

### Quiet mode

Removing the `-q` flag, we can see the systemd-run output:

```
$ playpen -q date
Running as unit: run-u821.service; invocation ID: 5577f88c2d304ae3aee5607965cd0eed
Press ^] three times within 1s to disconnect TTY.
Sat Dec 14 05:22:31 PM CET 2024
Finished with result: success
Main processes terminated with: code=exited/status=0
Service runtime: 5ms
CPU time consumed: 1ms
Memory peak: 256.0K
Memory swap peak: 0B
```

Internally, `playpen` uses `systemd-run` to launch the process. The
`--quiet` flag suppresses the systemd-run output. If `playpen` detects
that the output is a tty, it will include the `--pty` parameter to
`systemd-run`.

### Using `--` to separate `playpen` options from the command to run

You have to separate the `playpen` options from the command to run with
`--`:

```
$ playpen -q -m 300K -- date --rfc-3339 s
2024-12-14 17:29:40+01:00
```

### More complex example: calling Python

As shown earlier, you can start an interactive Python interpreter:

```
$ playpen -m 50M -q -- python3
Python 3.12.3 (main, Nov  6 2024, 18:32:19) [GCC 13.2.0] on linux
Type "help", "copyright", "credits" or "license" for more information.
>>> print(123)
123
>>> 
```

Simple example of running a python script with a 50 MB memory limit:

```
$ playpen -m 50M -q -- python3 -c "print(123)"
123
```

The `--quiet` flag suppresses the systemd-run wrapper output. This is what
you see if you omit the `--quiet` flag:

```
$ playpen -m 50M -- python3 -c "print(123)"
Running as unit: run-u814.service; invocation ID: d0523f2f13df45968dd05a48000dbb67
Press ^] three times within 1s to disconnect TTY.
123
Finished with result: success
Main processes terminated with: code=exited/status=0
Service runtime: 31ms
CPU time consumed: 26ms
Memory peak: 256.0K
Memory swap peak: 0B
```

The output above is nearly all stderr, which means that the actual
program output, in this case `123`, is on stdout and can therefore
be piped to another program. In the example below, we have our
Python layer emit `123 123`, and then we use `tr` to replace the
space with a `+`:

```
$ playpen -m 50M -- python3 -c "print('123 123')" | tr ' ' '+'
Running as unit: run-u816.service; invocation ID: 36a565c23f024a3e9b26b79d9695f6be
123+123
Finished with result: success
Main processes terminated with: code=exited/status=0
Service runtime: 14ms
CPU time consumed: 14ms
Memory peak: 512.0K
Memory swap peak: 0B
```

Note that the `tr` operated only on the stdout.

The above example has an important difference from the first example: the
message about `Press ^] three times within 1s to disconnect TTY` is not
printed. This is because the `playpen` command is no longer attached to a tty
on its stdout.

### Pipelines

You can use `playpen` in a pipeline.

```python
# add_days.py
import sys
from datetime import datetime as dt
from datetime import timedelta

for line in sys.stdin:
    value = dt.fromisoformat(line.strip())
    output = value + timedelta(days=int(sys.argv[1]))
    print(output.isoformat())
```

Now we can use it in a pipeline:

```
$ echo $(date --rfc-3339 s) \
      | playpen -m 50M -q --  python3 add_days.py 10 \
      | tr '[0-9]' 'X'
XXXX-XX-XXTXX:XX:XX+XX:XX
```

Of course, our script is never going to hit the memory limit, but it's
a good example of how to use `playpen` in a pipeline.


## Dependencies

This only works on Linux and requires the `systemd` service manager.
In particular, it uses the `systemd-run` command to launch the
processes in a cgroup with the given limits. As such, `playpen`
is a shallow wrapper around `systemd-run`.

## Recipes

### cargo build

Use the `cargo` profile inside a Rust project. This sets 2G memory, 300% CPU, and grants access to `~/.cargo` (for caching and a shared build directory) and `~/.rustup` (for access to the rust toolchain), while the rest of the home directory is hidden by `ProtectHome=tmpfs`.

```
playpen --profile cargo -- cargo build
```

Override the memory limit for a large project:

```
playpen --profile cargo -m 32G -- cargo build
```

### npm install

Use the `npm` profile for Node.js projects. It sets 1G memory, 200% CPU, and grants access to npm/yarn/pnpm caches.

```
playpen --profile npm -- npm install
```

### pytest

Use the `pytest` profile for Python testing. It sets 512M memory, 200% CPU, and grants read-only access to user-installed Python packages.

```
playpen --profile pytest -- pytest
```

### Running an AI coding agent

Use the `coding-agent` profile, then add the config directories for your specific agent. The profile hides the entire home directory by default, then selectively exposes git config and SSH keys needed for git operations.

```
# Claude Code
playpen --profile coding-agent --rw ~/.claude --ro ~/.claude.json -- claude

# OpenAI Codex
playpen --profile coding-agent --rw ~/.codex -- codex

# Gemini CLI
playpen --profile coding-agent --rw ~/.gemini -- gemini

# pi
playpen --profile coding-agent --rw ~/.pi -- pi
```

### Interactive shell session

Use the `shell` profile for an interactive terminal session. Home is read-only (all config visible), with write access granted to `~/.local/share`, `~/.cache`, and `~/.local/bin`.

```
playpen --profile shell -- bash
```

## Code Coverage

This project uses [`cargo-llvm-cov`](https://github.com/taiki-e/cargo-llvm-cov) for LLVM source-based code coverage. It measures coverage from both unit tests and integration tests in a single combined report.

### Prerequisites

```bash
cargo install cargo-llvm-cov
```

### Quick commands

Two convenience aliases are defined in `.cargo/config.toml`:

```bash
# Generate lcov.info and print a terminal summary
# (runs all tests under instrumentation, merges results)
cargo coverage

# Generate an HTML report and open it in your browser
# (line-by-line highlighting in target/llvm-cov/html)
cargo coverage-html
```

Both aliases instrument the entire workspace (`--all-features --workspace`), so all unit and integration test coverage is aggregated automatically.

### CI

A GitHub Actions workflow (`.github/workflows/coverage.yml`) generates `lcov.info` on every push/PR to `main` and uploads it as a build artifact.
