## Final Profile Selection (v5)

For the initial implementation, we recommend these 9 profiles:

1. **cargo** - Rust projects (2G memory, 300% CPU, swap disabled)
2. **npm** - Node.js projects (1G memory, 200% CPU, swap disabled)
3. **pytest** - Python testing (512M memory, 200% CPU, swap disabled)
4. **python** - General Python scripts (512M memory, 100% CPU, swap disabled)
5. **uv** - Python dependency management (256M memory, 200% CPU, swap disabled)
6. **go** - Go projects (512M memory, 300% CPU, swap enabled)
7. **make** - C/C++ make/cmake builds (2G memory, 300% CPU, swap disabled)
8. **coding_agent** - AI coding agents (4G memory, 200% CPU, swap disabled)
9. **shell** - Interactive shell/terminal sessions (4G memory, no CPU limit, swap disabled)

These cover the most common use cases while keeping the implementation simple.

**Decision**: A `default` profile was considered but rejected—running without
`--profile` already means "no limits", which is the existing behavior and should
remain unchanged for backward compatibility.

## Profile Definitions (v5 - Code Format)

### Struct Design: `protect_home` replaces `current_dir_only`

The v3 `current_dir_only: bool` only supported one protection mode (ProtectHome=tmpfs).
The new profiles need a different mode: `coding_agent` uses ProtectHome=tmpfs (like
the tool profiles), but `shell` uses ProtectHome=read-only (see all, write restricted).

The only difference between these modes is the `ProtectHome` value. Everything else
(PrivateTmp, PrivateDevices, ProtectKernelTunables, ProtectControlGroups, BindPaths=CWD)
is the same. So `current_dir_only: bool` becomes `protect_home: &'static str`:

| `protect_home` | systemd value | Home visibility | Use case |
|---|---|---|---|
| `"tmpfs"` | `ProtectHome=tmpfs` | Home hidden, mount what you need | Tool & agent profiles |
| `"read-only"` | `ProtectHome=read-only` | Home visible but read-only | Shell profile |
| `"no"` | `ProtectHome=no` | Home fully accessible | (not used by any built-in profile) |

All profiles with `protect_home` set also apply the rest of the lockdown base
(PrivateTmp=yes, PrivateDevices=yes, ProtectKernelTunables=yes,
ProtectControlGroups=yes, BindPaths=CWD). This is the same behavior as the
> ## Documentation Index
> Fetch the complete documentation index at: https://platform.kimi.ai/docs/llms.txt
> Use this file to discover all available pages before exploring further.

# Model Parameter Reference

export const DocTable = ({columns = [], rows = []}) => {
  return <div className="doc-table-wrap">
      <table className="doc-table">
        {columns.length > 0 ? <colgroup>
            {columns.map((column, index) => <col key={index} style={column.width ? {
    width: column.width
  } : undefined} />)}
          </colgroup> : null}
        <thead>
          <tr>
            {columns.map((column, index) => <th key={index}>{column.title}</th>)}
          </tr>
        </thead>
        <tbody>
          {rows.map((row, rowIndex) => <tr key={rowIndex}>
              {row.map((cell, cellIndex) => <td key={cellIndex}>{cell}</td>)}
            </tr>)}
        </tbody>
      </table>
    </div>;
};

Different model families have different defaults and constraints for Chat Completions API parameters. For the full model list, see the [Model List](/models).

## Parameter Comparison

<DocTable
  columns={[
{ title: "Parameter", width: "18%" },
{ title: "kimi-k2.6", width: "18%" },
{ title: "kimi-k2 series", width: "20%" },
{ title: "kimi-k2-thinking series", width: "24%" },
{ title: "moonshot-v1 series", width: "20%" },
]}
  rows={[
[<code>temperature</code>, <strong>Cannot be modified</strong>, "0.6", "1.0", "0.0"],
[<code>top_p</code>, <>0.95 <strong>Cannot be modified</strong></>, "1.0", "1.0", "1.0"],
[<code>n</code>, <>1 <strong>Cannot be modified</strong></>, "1 (max 5)", "1 (max 5)", "1 (max 5)"],
[<code>presence_penalty</code>, <>0 <strong>Cannot be modified</strong></>, "0 (modifiable)", "0 (modifiable)", "0 (modifiable)"],
[<code>frequency_penalty</code>, <>0 <strong>Cannot be modified</strong></>, "0 (modifiable)", "0 (modifiable)", "0 (modifiable)"],
[<code>thinking</code>, "Supported", "—", "—", "—"],
]}
/>

<Note>
  When `temperature` is close to 0, `n` can only be 1. Otherwise, the API returns `invalid_request_error`.
</Note>

## Kimi K2.6 — thinking Parameter

Kimi K2.6 supports the `thinking` parameter to control whether deep thinking is enabled. Accepts `{"type": "enabled"}` or `{"type": "disabled"}`.

Since the OpenAI SDK doesn't have a native `thinking` parameter, use `extra_body`:

<CodeGroup>
  ```python Python theme={null}
  completion = client.chat.completions.create(
      model="kimi-k2.6",
      messages=[
          {"role": "user", "content": "Hello"}
      ],
      extra_body={
          "thinking": {"type": "disabled"}
      },
      max_tokens=1024*32,
  )
  ```

  ```bash cURL theme={null}
  curl https://api.moonshot.ai/v1/chat/completions \
    -H "Content-Type: application/json" \
    -H "Authorization: Bearer $MOONSHOT_API_KEY" \
    -d '{
      "model": "kimi-k2.6",
      "messages": [
        {"role": "user", "content": "Hello"}
      ],
      "thinking": {"type": "disabled"}
    }'
  ```
</CodeGroup>
existing `--current-dir-only` preset, just with a configurable ProtectHome mode.

### Code

```rust
#[derive(Debug, Clone, Copy)]
struct Profile {
    name: &'static str,
    description: &'static str,
    memory_limit: Option<&'static str>,
    cpu_quota: Option<&'static str>,
    memory_swap_max: Option<&'static str>,
    /// ProtectHome mode: "tmpfs" (hide home), "read-only" (visible but read-only),
    /// "no" (no protection). When set, also applies PrivateTmp, PrivateDevices,
    /// ProtectKernelTunables, ProtectControlGroups, and BindPaths=CWD.
    protect_home: &'static str,
    /// Read-write bind paths (expanded at runtime from shell env vars)
    rw_paths: &'static [&'static str],
    /// Read-only bind paths (expanded at runtime from shell env vars)
    ro_paths: &'static [&'static str],
}

const DEFAULT_CPU_QUOTA_PERIOD: &str = "100ms";

const PROFILES: &[Profile] = &[
    Profile {
        name: "cargo",
        description: "Rust/Cargo builds and tests",
        memory_limit: Some("2G"),
        cpu_quota: Some("300%"),
        memory_swap_max: Some("0"),
        protect_home: "tmpfs",
        // $HOME/.cargo: registry, git cache, build cache (needs write for builds)
        // $HOME/.rustup: toolchain installations (read-only is sufficient)
        rw_paths: &["$HOME/.cargo"],
        ro_paths: &["$HOME/.rustup"],
    },
    Profile {
        name: "npm",
        description: "Node.js npm/yarn/pnpm build and test",
        memory_limit: Some("1G"),
        cpu_quota: Some("200%"),
        memory_swap_max: Some("0"),
        protect_home: "tmpfs",
        // $HOME/.npm: npm cache (needs write for installs)
        // $HOME/.cache/yarn: yarn cache (needs write for installs)
        // $HOME/.local/share/pnpm: pnpm store (needs write for installs)
        // $HOME/.local/share/fnm: fnm node version manager binaries
        // /run/user/$UID: fnm runtime symlinks
        rw_paths: &["$HOME/.npm", "$HOME/.cache/yarn", "$HOME/.local/share/pnpm"],
        ro_paths: &["$HOME/.local/share/fnm", "/run/user/$UID"],
    },
    Profile {
        name: "pytest",
        description: "Python pytest (single or parallel mode)",
        memory_limit: Some("512M"),
        cpu_quota: Some("200%"),
        memory_swap_max: Some("0"),
        protect_home: "tmpfs",
        rw_paths: &[],
        ro_paths: &["$HOME/.local/lib"],
    },
    Profile {
        name: "python",
        description: "General Python script execution",
        memory_limit: Some("512M"),
        cpu_quota: Some("100%"),
        memory_swap_max: Some("0"),
        protect_home: "tmpfs",
        rw_paths: &[],
        ro_paths: &["$HOME/.local/lib"],
    },
    Profile {
        name: "uv",
        description: "Python uv dependency management",
        memory_limit: Some("256M"),
        cpu_quota: Some("200%"),
        memory_swap_max: Some("0"),
        protect_home: "tmpfs",
        // $HOME/.cache/uv: download cache (needs write)
        // $HOME/.local/share/uv: uv python installs (needs write)
        rw_paths: &["$HOME/.cache/uv", "$HOME/.local/share/uv"],
        ro_paths: &[],
    },
    Profile {
        name: "go",
        description: "Go builds and tests",
        memory_limit: Some("512M"),
        cpu_quota: Some("300%"),
        memory_swap_max: None, // Allow swap — Go linker may need it
        protect_home: "tmpfs",
        // $HOME/go: GOPATH — module cache, bin (needs write for go install)
        // $HOME/.cache/go-build: build cache (needs write)
        rw_paths: &["$HOME/go", "$HOME/.cache/go-build"],
        ro_paths: &[],
    },
    Profile {
        name: "make",
        description: "C/C++ make/cmake builds",
        memory_limit: Some("2G"),
        cpu_quota: Some("300%"),
        memory_swap_max: Some("0"),
        protect_home: "tmpfs",
        rw_paths: &[],
        ro_paths: &[],
    },
    Profile {
        name: "coding_agent",
        description: "AI coding agent (claude, codex, gemini, pi, etc.)",
        memory_limit: Some("4G"),
        cpu_quota: Some("200%"),
        memory_swap_max: Some("0"),
        protect_home: "tmpfs",
        // Common paths ALL coding agents need:
        //   $HOME/.gitconfig: git identity for commits (ro)
        //   $HOME/.ssh: git push/pull auth (ro — contains private keys, see notes)
        // Agent-specific paths are NOT included because they vary per agent.
        // Users add them with --rw/--ro flags. Examples:
        //   Claude Code:  --rw $HOME/.claude --ro $HOME/.claude.json
        //   Codex:        --rw $HOME/.codex
        //   Gemini:       --rw $HOME/.gemini
        //   pi:           --rw $HOME/.pi
        rw_paths: &[],
        ro_paths: &["$HOME/.gitconfig", "$HOME/.ssh"],
    },
    Profile {
        name: "shell",
        description: "Interactive shell/terminal session (read-only home)",
        memory_limit: Some("4G"),
        cpu_quota: None, // No CPU limit — user may run anything
        memory_swap_max: Some("0"),
        protect_home: "read-only",
        // With ProtectHome=read-only, all of home is VISIBLE but read-only.
        // These rw_paths grant write access to commonly-needed subdirectories.
        // $HOME/.local/share: app data, shell history (fish, atuin, etc.)
        // $HOME/.cache: build caches, download caches
        // $HOME/.local/bin: user-installed tools
        rw_paths: &["$HOME/.local/share", "$HOME/.cache", "$HOME/.local/bin"],
        ro_paths: &[],
    },
];
```

### Notes on Values:
- Memory values use systemd format (K, M, G suffixes)
- CPU quota uses systemd format with `%` suffix (e.g., `"300%"` = 300% of one CPU core)
- `cpu_quota_period` is always `100ms` — stored as `DEFAULT_CPU_QUOTA_PERIOD` constant,
  not a per-profile field. If a future tool needs a different period, we'll add it then.
- `memory_swap_max` controls whether swap is allowed (`Some("0")` = no swap, `None` = allow swap)
- `protect_home` sets the `ProtectHome` systemd property. When set to anything other
  than `"no"`, the profile also applies the lockdown base: PrivateTmp=yes,
  PrivateDevices=yes, ProtectKernelTunables=yes, ProtectControlGroups=yes,
  BindPaths=CWD. This is the same base as the existing `--current-dir-only` preset.
- `rw_paths` and `ro_paths` contain environment variable references like `$HOME/.cargo`
  that are expanded at runtime using shell-style expansion.
- `Copy` is derived since all fields are `&'static str` or `&'static [&'static str]`
  (no heap allocation).

### `coding_agent` Profile Design Notes

**Threat model**: An AI coding agent with filesystem access could be prompt-injected
to (1) read secrets and exfiltrate them, or (2) write destructive changes to
important files. The profile protects against both.

**Why ProtectHome=tmpfs**: Hides all of home by default, then selectively exposes
only what the agent needs. This prevents reading secrets in dotfiles (e.g.,
`~/.aws/credentials`, `~/.env`, `~/.kube/config`) unless explicitly mounted back.

**Why `~/.ssh` is read-only despite containing private keys**: Coding agents need
SSH for git operations (push/pull). Without `~/.ssh`, git auth breaks. Making it
read-only prevents the agent from writing new keys or modifying ssh config, while
still allowing git to function. The read access to private keys is a trade-off —
a truly malicious agent with network access could exfiltrate them, but preventing
that requires network isolation, which is a separate feature.

**Why agent-specific paths are NOT in the profile**: Different agents store config
in different directories (Claude: `~/.claude`, Codex: `~/.codex`, Gemini: `~/.gemini`,
pi: `~/.config/pi-coding-agent`). Hardcoding all of them would expose every agent's
config to every other agent. Instead, the user adds only the paths their specific
agent needs. This is a deliberate security choice: mount only what's needed.

**Usage examples**:
```bash
# Claude Code
playpen --profile coding_agent --rw ~/.claude --ro ~/.claude.json -- claude

# OpenAI Codex
playpen --profile coding_agent --rw ~/.codex -- codex

# Gemini CLI
playpen --profile coding_agent --rw ~/.gemini -- gemini

# pi
playpen --profile coding_agent --rw ~/.pi -- pi
```

### `shell` Profile Design Notes

**Threat model**: Accidental destructive writes in an interactive terminal session
(e.g., `rm -rf` in the wrong directory). The user IS the operator, so read-protection
of secrets is not the concern — preventing accidental writes is.

**Why ProtectHome=read-only**: A shell needs to read all sorts of config (shell rc
files, starship config, atuin history, aliases, gitconfig, etc). With ProtectHome=tmpfs,
we'd have to list every possible config directory. With read-only, all of home is
visible automatically, but the shell can't accidentally write to `~/.bashrc`,
`~/.ssh/`, `~/.gitconfig`, etc.

**Write access to `~/.local/share`, `~/.cache`, `~/.local/bin`**: These are the
subdirectories that shells and CLI tools commonly need to write to during normal
operation (shell history, application data, build caches, tool installs). The
profile grants write access to these while keeping the rest of home read-only.

**No CPU limit**: A terminal session may run anything — builds, compiles, data
processing. Imposing a CPU limit would break too many workflows. Memory is still
limited to prevent runaway processes from locking up the system.

**Usage example**:
```bash
# Wrap an entire bash session
playpen --profile shell -- bash

# Or as a terminal emulator command
# (alacritty, kitty, etc. can be configured to launch playpen)
playpen --profile shell -- fish
```

### Path Expansion Logic

Profile path entries like `$HOME/.cargo` must be expanded at runtime. Implementation:

```rust
fn expand_path(path: &str) -> String {
    // Expand $HOME, $UID, and other env vars
    shellexpand::env(path).unwrap_or_else(|_| path.into()).into_owned()
}
```

This requires adding the `shellexpand` crate as a dependency. If we want to avoid
the dependency, we can do simple manual expansion for `$HOME` and `$UID` only
(since those are the only vars used in the profile definitions).

### Path Existence Check

Before adding a profile path to the `systemd-run` command, verify the source path
exists. If it does not exist, skip it silently — do not add the bind parameter.
This handles setup-specific paths (e.g., `$HOME/.cache/yarn` for users who don't
have Yarn installed) without causing `systemd-run` to fail.

```rust
fn add_bind_path(parts: &mut Vec<String>, path: &str, read_only: bool) {
    let expanded = expand_path(path);
    if std::path::Path::new(&expanded).exists() {
        let flag = if read_only { "BindReadOnlyPaths" } else { "BindPaths" };
        parts.push(format!("-p{}={}", flag, expanded));
    }
    // Silently skip non-existent paths
}
```

### Symlink Considerations

With ProtectHome=tmpfs, home directories are replaced by an empty tmpfs. If a
path like `~/.claude.json` is a symlink (e.g., `~/.claude.json -> stowfiles/.claude.json`),
bind-mounting only `~/.claude.json` will create a broken symlink because the target
(`stowfiles/.claude.json`) is also under the hidden `/home` tree and not bind-mounted.

**Mitigation**: Users with symlinked dotfiles need to also bind-mount the symlink
target directory. For example:
```bash
playpen --profile coding_agent --ro ~/.claude.json --ro ~/stowfiles -- claude
```
This is documented in the README rather than handled automatically, since symlink
resolution is filesystem-specific and trying to auto-resolve could expose
unintended directories.

### Resolved Design Decisions:
- **No `default` profile**: Running without `--profile` means no limits (existing behavior)
- **Profiles include filesystem access limits**: This is a core motivation for profiles —
  users shouldn't have to figure out what paths each tool needs. Profiles bundle the
  resource limits AND the filesystem access the tool typically requires.
- **`protect_home` replaces `current_dir_only`**: The `current_dir_only: bool` field
  only supported ProtectHome=tmpfs. The new `protect_home: &'static str` field supports
  tmpfs, read-only, and no-protection modes, enabling the shell profile's "read-only
  home" approach while keeping the same lockdown base for all other profiles.
- **Lockdown base is always applied when `protect_home` is set**: Regardless of the
  ProtectHome mode, the same base protections are applied (PrivateTmp, PrivateDevices,
  ProtectKernelTunables, ProtectControlGroups, BindPaths=CWD). Only the ProtectHome
  value differs.
- **No separate sandboxing in profiles**: Additional sandboxing flags (`private_tmp`,
  `protect_home`, etc.) remain independent CLI flags and are not part of profiles.
  Rationale: finer sandboxing is a security decision orthogonal to what a tool
  needs to function.
- **CPU quota format**: Store as systemd-ready strings like `"300%"`, not bare `"300"`.
  This matches what `systemd-run` expects for `-pCPUQuota=` and avoids format conversion bugs.
- **CPU quota period**: Always `100ms` (stored as a constant). Not a per-profile field
  since no profile needs a different value. YAGNI.
- **`MemorySwapMax`**: Explicitly included in profile struct so each profile can control
  it independently (e.g., Go allows swap, others don't).
- **`--memory-swap-max` CLI parameter**: Exposes `MemorySwapMax` as a direct CLI
  parameter, e.g. `--memory-swap-max 0` (no swap) or `--memory-swap-max 1G` (allow
  up to 1G swap). This follows the same override pattern as every other setting:
  later arguments override earlier ones. No special-casing needed.
  When neither `--memory-swap-max` nor a profile is active, existing behavior applies
  (swap disabled when `-m` is given).
- **Agent-specific paths NOT in coding_agent profile**: Different agents store config
  in different directories. Including all agent configs would expose every agent's
  config to every other agent. Users add only what their specific agent needs via
  `--rw`/`--ro`. This is a deliberate security trade-off.
- **No `list-profiles` subcommand or flag**: Profiles are documented in `--help`
  (as possible values for `--profile`) and in the README. No extra command surface.
- **`--dry-run` flag** (not `--show-config`): Shows the resolved `systemd-run` command
  without executing. `--dry-run` is more conventional and widely understood than
  `--show-config`, which sounds like it displays configuration rather than the
  full command preview.

## Parameter Precedence: Rightmost Wins

All arguments are processed **left to right** on the command line. Each argument
modifies the running state. The **rightmost** (last) occurrence of any setting wins,
regardless of whether it comes from a simple flag or a compound parameter (profile
or preset).

### Examples

```bash
# Explicit flag AFTER profile: explicit wins
playpen --profile cargo -m 4G -- cargo build
# Result: MemoryMax=4G (profile's 2G is overridden)

# Profile AFTER explicit flag: profile wins
playpen -m 4G --profile cargo -- cargo build
# Result: MemoryMax=2G (profile overrides the earlier 4G)

# Preset AFTER profile: preset wins
playpen --profile shell --current-dir-only -- bash
# Result: ProtectHome=tmpfs (preset overrides profile's read-only)

# Profile AFTER preset: profile wins
playpen --current-dir-only --profile shell -- bash
# Result: ProtectHome=read-only (profile overrides preset's tmpfs)

# Multiple explicit flags: last one wins
playpen --private-tmp=true --private-tmp=false -- echo hi
# Result: PrivateTmp=no
```

### Implementation Model

The command builder maintains a map of systemd property names to their current
values. As arguments are processed in command-line order:

1. **Simple flags** (`-m 4G`, `--private-tmp=false`) set one or more entries directly.
2. **Compound flags** (`--profile cargo`, `--current-dir-only`) expand into their
   constituent entries and set them all at once.
3. **Append-only lists** (`--rw`, `--ro`, `--inaccessible`, `--setenv`) accumulate
   values rather than overwriting.
4. After all arguments are processed, the final map is emitted as `-pKey=Value`
   arguments to `systemd-run`.

This gives "later overrides earlier" semantics naturally without any special-casing.

## Implementation Plan

### Step 1: Add Dependencies
- Add `shellexpand` crate to `Cargo.toml` for path expansion
  (or implement simple `$HOME`/`$UID` expansion manually)

### Step 2: Add Profile Struct and Constants
- Add `Profile` struct after imports in main.rs (with `protect_home` field)
- Add `DEFAULT_CPU_QUOTA_PERIOD` constant
- Add `PROFILES` array with all 9 profile definitions
- Add `fn expand_path(path: &str) -> String` helper
- Add `fn find_profile(name: &str) -> Option<&'static Profile>` helper

### Step 3: Add CLI Arguments
- Add `--profile <NAME>` CLI argument to `Run` struct
- Add `--memory-swap-max` CLI argument to `Run` struct
- Add `--dry-run` CLI argument to `Run` struct
- Keep top-level `#[derive(Parser)]` on `Run` (no subcommands, backward compatible)
- In `--profile` help text, list all 9 profile names so users can discover them
  without a separate command

### Step 4: Implement Command Builder with Rightmost-Wins Semantics

Refactor command building to use a stateful builder processed in argument order:

```rust
struct SystemdProps {
    // Single-value properties: last write wins
    props: HashMap<String, String>,
    // Multi-value properties: accumulate
    lists: HashMap<String, Vec<String>>,
}
```

Process arguments in order:
1. Start with empty `SystemdProps`
2. If `--profile <name>` appears, look up the profile and apply all its settings
   to `SystemdProps` (memory, CPU, swap, protect_home mode, rw_paths, ro_paths)
3. If explicit `-m`, `-c`, `--memory-swap-max`, `--private-tmp`, etc. appear,
   apply them to `SystemdProps` (overwriting whatever was there)
4. If `--current-dir-only` appears, apply its lockdown base (overwriting previous
   protect_home and related settings)
5. Collect `--rw`, `--ro`, `--inaccessible` into lists
6. After all arguments processed, emit the final `systemd-run` command from
   `SystemdProps`

**Unknown profile handling**: If `--profile` names an unknown profile, print an
error message that lists all valid profile names with their descriptions, then
exit with code 1.

### Step 5: Implement Filesystem Path Logic
- Expand profile `rw_paths` and `ro_paths` using `expand_path()`
- **Skip non-existent paths** — check `Path::exists()` and omit if missing
- Add remaining paths to the appropriate lists (`BindPaths` or `BindReadOnlyPaths`)
- User's explicit `--rw` / `--ro` paths are also added to the lists (in addition to
  profile paths — both accumulate since they are list properties)
- When no profile is active, existing behavior applies (including `--current-dir-only`)

### Step 6: Update Command Building Logic
- When `MemoryMax` is set in props: emit `-pMemoryMax=<value>`
- When `MemorySwapMax` is set in props: emit `-pMemorySwapMax=<value>`
  - If no profile and no explicit `--memory-swap-max`: keep existing behavior
    (set `MemorySwapMax=0` when `-m` is given)
- When `CPUQuota` is set in props: emit `-pCPUQuota=<value>` and
  `-pCPUQuotaPeriodSec=100ms`
- When `ProtectHome` is set in props (not "no"): emit lockdown base props
  (PrivateTmp, PrivateDevices, ProtectKernelTunables, ProtectControlGroups,
  ProtectHome, BindPaths=CWD)
- Emit all accumulated `BindPaths`, `BindReadOnlyPaths`, `InaccessiblePaths`
- When `--dry-run` is set: print the full `systemd-run` command and exit 0

### Step 7: Add Integration Tests
- Add tests using `assert_cmd` (already a dev-dependency):
  - Profile sets correct systemd properties
  - Profile expands `$HOME` in path entries
  - Non-existent profile paths are silently skipped
  - Explicit flags override profile resource values when after profile
  - Profile overrides explicit flags when after them (rightmost-wins)
  - `--memory-swap-max` overrides profile swap setting
  - Profile + explicit `--rw`/`--ro` paths both accumulate
  - Profile + sandboxing flags interact correctly with rightmost-wins
  - Profile + `--quiet` works
  - Profile + `--capture-env` works
  - `--dry-run` prints resolved command without executing
  - No `--profile` produces identical output to current behavior
  - Go profile allows swap (no `MemorySwapMax=0` set)
  - Non-Go profiles set `MemorySwapMax=0`
  - `coding_agent` profile uses ProtectHome=tmpfs
  - `shell` profile uses ProtectHome=read-only
  - Unknown profile returns exit code 1 and lists valid profiles

### Step 8: Update Help Text and README
- In `--profile` help text, enumerate all 9 profiles with one-line descriptions
- Document `--dry-run` and `--memory-swap-max` flags
- Document rightmost-wins precedence with examples
- Add README section about profiles with examples:
  - `playpen --profile cargo -- cargo build`
  - `playpen --profile npm -- npm install`
  - Override example: `playpen --profile cargo -m 4G -- cargo build`
  - Swap override: `playpen --profile cargo --memory-swap-max 1G -- cargo build`
  - Add path example: `playpen --profile cargo --rw $HOME/.ccache -- cargo build`
  - Coding agent examples (one per agent)
  - Shell example: `playpen --profile shell -- bash`
  - Show `--dry-run` for debugging
  - Document symlink caveat with ProtectHome=tmpfs
  - Document that non-existent profile paths are skipped

## Testing Checklist

### Basic Profile Usage
- [ ] `playpen --profile cargo -- cargo build` works
- [ ] `playpen --profile npm -- npm run build` works
- [ ] `playpen --profile pytest -- pytest` works
- [ ] `playpen --profile python -- python3 script.py` works
- [ ] `playpen --profile uv -- uv pip install` works
- [ ] `playpen --profile go -- go build` works
- [ ] `playpen --profile make -- make` works
- [ ] `playpen --profile coding_agent --rw ~/.claude -- claude` works
- [ ] `playpen --profile shell -- bash` works

### Filesystem Access
- [ ] Cargo profile grants rw to `$HOME/.cargo` and ro to `$HOME/.rustup`
- [ ] npm profile grants rw to npm/yarn/pnpm caches and ro to fnm/node prefix
- [ ] pytest profile grants ro to `$HOME/.local/lib` (site-packages)
- [ ] python profile grants ro to `$HOME/.local/lib` (site-packages)
- [ ] uv profile grants rw to `$HOME/.cache/uv` and `$HOME/.local/share/uv`
- [ ] go profile grants rw to `$HOME/go` and `$HOME/.cache/go-build`
- [ ] make profile applies lockdown base with no extra paths
- [ ] coding_agent grants ro to `$HOME/.gitconfig` and `$HOME/.ssh`
- [ ] coding_agent does NOT include agent-specific config dirs in profile
- [ ] shell profile uses ProtectHome=read-only (all of home visible)
- [ ] shell profile grants rw to `$HOME/.local/share`, `$HOME/.cache`, `$HOME/.local/bin`
- [ ] `$HOME` in profile paths is expanded at runtime
- [ ] `$UID` in profile paths is expanded at runtime
- [ ] Profile paths + explicit `--rw`/`--ro` paths are both applied
- [ ] Profile with no extra paths still applies lockdown base
- [ ] Non-existent profile paths are silently skipped (no systemd-run failure)

### coding_agent Specific
- [ ] Claude Code: `playpen --profile coding_agent --rw ~/.claude --ro ~/.claude.json -- claude`
- [ ] Codex: `playpen --profile coding_agent --rw ~/.codex -- codex`
- [ ] Gemini: `playpen --profile coding_agent --rw ~/.gemini -- gemini`
- [ ] Agent cannot read `~/.aws/credentials` (not bind-mounted)
- [ ] Agent cannot read `~/.kube/config` (not bind-mounted)
- [ ] Agent CAN read `~/.ssh/` (bind-mounted read-only)
- [ ] Agent CAN read `~/.gitconfig` (bind-mounted read-only)
- [ ] Agent CAN write to CWD (bind-mounted read-write)
- [ ] Agent CANNOT write to `~/.ssh/` (read-only)

### shell Specific
- [ ] Shell can read all of home (ProtectHome=read-only)
- [ ] Shell can write to CWD
- [ ] Shell can write to `$HOME/.local/share`
- [ ] Shell can write to `$HOME/.cache`
- [ ] Shell can write to `$HOME/.local/bin`
- [ ] Shell CANNOT write to `$HOME/.bashrc` (home is read-only)
- [ ] Shell CANNOT write to `$HOME/.ssh/` (home is read-only)
- [ ] Shell has no CPU limit (cpu_quota is None)

### Rightmost-Wins Precedence
- [ ] `playpen --profile cargo -m 4G` uses 4G (explicit after profile)
- [ ] `playpen -m 4G --profile cargo` uses 2G (profile after explicit)
- [ ] `playpen --profile shell --current-dir-only` uses tmpfs (preset after profile)
- [ ] `playpen --current-dir-only --profile shell` uses read-only (profile after preset)
- [ ] `playpen --private-tmp=true --private-tmp=false` uses false (last explicit wins)

### Swap Behavior
- [ ] Go profile allows swap (no `MemorySwapMax=0` set)
- [ ] Non-Go profiles set `MemorySwapMax=0`
- [ ] `--memory-swap-max 0` overrides Go profile to disable swap
- [ ] `--memory-swap-max 1G` overrides cargo profile to allow swap
- [ ] No profile + `-m` still sets `MemorySwapMax=0` (existing behavior)
- [ ] No profile + `-m` + `--memory-swap-max 1G` allows swap

### Error Handling
- [ ] Unknown profile shows error and lists valid profiles
- [ ] `playpen --profile` without a name gives clap error
- [ ] `playpen --profile cargo` without a command gives appropriate error

### Profile + Existing Features
- [ ] Profile + `--quiet` works correctly
- [ ] Profile + `--capture-env` works correctly
- [ ] Profile + `--current-dir-only` works correctly with rightmost-wins
- [ ] Profile + sandboxing flags (`--private-tmp`, `--protect-home`, etc.) work
- [ ] Profile + path controls (`--rw`, `--ro`, `--inaccessible`) work
- [ ] TTY detection still works with profiles

### Backward Compatibility
- [ ] Running without `--profile` produces identical `systemd-run` command as before
- [ ] All existing CLI flags work unchanged when no profile is specified
- [ ] `-m` and `-c` short flags still work
- [ ] `--current-dir-only` still works when no profile is specified

### Debugging
- [ ] `--dry-run` prints the full `systemd-run` command without executing
- [ ] `--dry-run` with profile shows resolved limits and expanded paths
- [ ] `--dry-run` with profile + overrides shows final resolved values

### Automated Integration Tests (assert_cmd)
- [ ] Test profile names appear in `--profile` help text
- [ ] Test unknown profile returns exit code 1 and lists valid names
- [ ] Test no-profile behavior is unchanged
- [ ] Test `--dry-run` output with profile
- [ ] Test profile path expansion
- [ ] Test non-existent paths are skipped (path not in dry-run output)
- [ ] Test rightmost-wins: explicit flag after profile overrides it
- [ ] Test rightmost-wins: profile after explicit flag overrides it
- [ ] Test coding_agent profile has ProtectHome=tmpfs in dry-run output
- [ ] Test shell profile has ProtectHome=read-only in dry-run output

## Profile Path Reference

This section documents the filesystem paths each tool typically needs,
for reference when reviewing or adjusting profile defaults.

### cargo
| Path | Access | Purpose |
|------|--------|---------|
| `$HOME/.cargo` | rw | Registry index, git cache, build cache, bin installs |
| `$HOME/.rustup` | ro | Toolchain installations (rustc, cargo, rustfmt, etc.) |
| CWD | rw | Source code and target/ output (via lockdown base) |

### npm
| Path | Access | Purpose |
|------|--------|---------|
| `$HOME/.npm` | rw | npm cache |
| `$HOME/.cache/yarn` | rw | Yarn cache |
| `$HOME/.local/share/pnpm` | rw | pnpm store |
| `$HOME/.local/share/fnm` | ro | fnm node version manager binaries |
| `/run/user/$UID` | ro | fnm runtime symlinks |
| CWD | rw | Project files and node_modules (via lockdown base) |

**Note**: Users with nvm instead of fnm will need `--ro $HOME/.nvm`. Users with
system node will need no extra paths. The npm prefix path varies by setup; fnm
users need `/run/user/$UID` for the symlinked node binary.

### pytest
| Path | Access | Purpose |
|------|--------|---------|
| `$HOME/.local/lib` | ro | User-installed Python packages (--user installs) |
| CWD | rw | Test files, .pytest_cache, .venv (via lockdown base) |

**Note**: Most pytest usage works within the project directory. Virtual envs
(.venv/) are typically in the project dir and already accessible via the
lockdown base. Only `--user` installed packages need `$HOME/.local/lib`.

### python
| Path | Access | Purpose |
|------|--------|---------|
| `$HOME/.local/lib` | ro | User-installed Python packages |
| CWD | rw | Scripts and data (via lockdown base) |

Same rationale as pytest.

### uv
| Path | Access | Purpose |
|------|--------|---------|
| `$HOME/.cache/uv` | rw | Download cache for wheels and sources |
| `$HOME/.local/share/uv` | rw | Managed Python installations |
| CWD | rw | Project files (via lockdown base) |

**Note**: uv stores its managed python installations separately from the system
python, and its cache separately from pip's cache.

### go
| Path | Access | Purpose |
|------|--------|---------|
| `$HOME/go` | rw | GOPATH: module cache (pkg/mod), bin installs |
| `$HOME/.cache/go-build` | rw | Build cache |
| CWD | rw | Source code (via lockdown base) |

**Note**: Go allows swap because the Go linker can have transient memory spikes
that exceed the 512M limit during linking of large binaries. Consider raising
the memory limit if swap-allowed builds are too slow.

### make
| Path | Access | Purpose |
|------|--------|---------|
| CWD | rw | Source and build output (via lockdown base) |

**Note**: Make/cmake builds typically work entirely within the project directory.
Users with ccache or distcc may need to add `--rw $HOME/.ccache` or similar.

### coding_agent
| Path | Access | Purpose |
|------|--------|---------|
| `$HOME/.gitconfig` | ro | Git identity (user.name, user.email) for commits |
| `$HOME/.ssh` | ro | SSH keys and config for git push/pull |
| CWD | rw | Project source code (via lockdown base) |

**Agent-specific paths (add via `--rw`/`--ro`)**:

| Agent | Paths | Access | Purpose |
|-------|-------|--------|---------|
| Claude Code | `$HOME/.claude` | rw | Sessions, settings, projects, skills, plugins, cache |
| Claude Code | `$HOME/.claude.json` | ro | Auth and top-level config |
| Codex | `$HOME/.codex` | rw | Codex config and auth |
| Gemini | `$HOME/.gemini` | rw | Auth (OAuth), settings |
| pi | `$HOME/.pi` | rw | pi config (path varies by installation) |

**Security notes**:
- The profile does NOT include agent-specific paths because different agents
  should not see each other's configs. Add only what your agent needs.
- `~/.ssh` is mounted read-only. The agent can read private keys (needed for
  git auth) but cannot write new keys or modify ssh config. Full secret
  protection requires network isolation, which is a separate feature.
- Cloud credential directories (`~/.aws`, `~/.gcp`, `~/.kube`, etc.) are NOT
  accessible — they're hidden by ProtectHome=tmpfs.
- Symlinked dotfiles may break if the symlink target is also under the hidden
  `/home` tree. Bind-mount the target directory too (see Symlink Considerations).

### shell
| Path | Access | Purpose |
|------|--------|---------|
| All of `$HOME` | read-only | All config, dotfiles, secrets visible but not writable |
| `$HOME/.local/share` | rw | App data, shell history (fish, atuin, etc.) |
| `$HOME/.cache` | rw | Build caches, download caches |
| `$HOME/.local/bin` | rw | User-installed tools |
| CWD | rw | Whatever directory the user is working in |
| `/tmp` | rw | Temporary files (via PrivateTmp) |

**Design rationale**:
- ProtectHome=read-only is used instead of tmpfs because a shell needs to read
  diverse config files (shell rc, starship, atuin, gitconfig, etc.) and listing
  every possible config directory is impractical.
- The three writable subdirectories cover the most common write needs during
  interactive sessions. If the user needs to write elsewhere, they can add `--rw`.
- No CPU limit is set because a terminal session may run arbitrary workloads.
  Memory is still limited to 4G to prevent runaway processes from locking the system.

## Current Status: Draft Complete (v5)

The plan incorporates all review findings. Remaining open questions:
- Should profiles eventually support custom user-defined profiles via config file?
  (e.g., `~/.config/playpen/profiles.toml`) — deferred to future feature.
- Should profiles include finer sandboxing presets in a future version? — deferred.
- Should we add a `--profile=none` escape hatch for users who have `--profile`
  in a shell alias? — consider for future, not blocking for v1.
- Should coding_agent eventually have sub-profiles (e.g., `coding_agent/claude`)
  that include agent-specific paths? — deferred. The `--rw`/`--ro` override
  approach is sufficient for v1 and avoids the security concern of exposing
  all agents' configs to whichever agent is running.

When ready, we'll:
1. Add shellexpand dependency (or manual expansion)
2. Implement the profile system in main.rs
3. Add integration tests
4. Update README
5. Update help text

---

*Last updated: 2026-04-25*
*Changelog:*
- *v5 — Removed `list-profiles` subcommand. Added "rightmost wins" positional
  precedence rule replacing explicit>profile hierarchy. Added path existence
  checks for profile bind paths. Simplified CLI surface (no subcommands).*
- *v4 — Added `coding_agent` and `shell` profiles. Replaced `current_dir_only: bool`
  with `protect_home: &'static str` to support both ProtectHome=tmpfs (tool/agent
  profiles) and ProtectHome=read-only (shell profile). Added `--memory-swap-max`
  CLI parameter. Added symlink considerations section. Added detailed threat model
  analysis and security notes for coding_agent. Added agent-specific path reference
  table. Expanded testing checklist with coding_agent and shell specific tests.*
- *v3 — Added filesystem access limits to profiles (rw_paths, ro_paths, protect_home).
  Changed `list-profiles` to a subcommand. Renamed `--show-config` to `--dry-run`.
  Removed per-profile `cpu_quota_period` field (always 100ms constant). Added
  `--memory-swap-max` CLI parameter so swap follows the same override pattern as
  all other settings. Added Profile Path Reference section. Added shellexpand
  dependency step.*
- *v2 — Fixed const/compile issue, added python+make profiles, added swap/period fields,
  merged description+notes, added --show-config, added integration tests, expanded
  testing checklist, resolved design decisions on sandboxing and default profile.*
