use anyhow::{bail, Context, Result};
use atty::Stream;
use clap::builder::BoolishValueParser;
use clap::ArgAction;
use clap::Parser;
use nix::unistd::execvp;
use std::ffi::CString;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

// ============ Profile Definitions ============

/// A named bundle of resource limits and filesystem access tuned for a
/// common workload (a Cargo build, a pytest run, etc.). A profile only
/// supplies a baseline; any explicit CLI flag overrides the matching field.
#[derive(Debug, Clone, Copy)]
struct Profile {
    name: &'static str,
    description: &'static str,
    memory_limit: Option<&'static str>,
    cpu_quota: Option<&'static str>,
    memory_swap_max: Option<&'static str>,
    protect_home: &'static str,
    protect_system: Option<&'static str>,
    /// Discover common agent state/tools and use an allowlisted environment.
    agent_access: bool,
    /// Network isolation opinion. `Some(true)` = `PrivateNetwork=yes`,
    /// `Some(false)` = `PrivateNetwork=no`, `None` = no opinion (the property
    /// is not emitted, leaving network available — systemd's default).
    ///
    /// Every built-in profile deliberately sets this to `None`: build tools
    /// (`cargo`, `npm`, `uv`, …) routinely fetch dependencies, so a profile
    /// that silently disabled network would produce "works directly, fails in
    /// playpen" surprises. Disabling network stays an explicit user choice via
    /// `--private-network`. The field exists so a future PR can opt individual
    /// profiles in once usage shows which are genuinely network-free.
    private_network: Option<bool>,
    rw_paths: &'static [&'static str],
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
        protect_system: None,
        agent_access: false,
        private_network: None,
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
        protect_system: None,
        agent_access: false,
        private_network: None,
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
        protect_system: None,
        agent_access: false,
        private_network: None,
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
        protect_system: None,
        agent_access: false,
        private_network: None,
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
        protect_system: None,
        agent_access: false,
        private_network: None,
        rw_paths: &["$HOME/.cache/uv", "$HOME/.local/share/uv"],
        ro_paths: &[],
    },
    Profile {
        name: "go",
        description: "Go builds and tests",
        memory_limit: Some("512M"),
        cpu_quota: Some("300%"),
        memory_swap_max: None,
        protect_home: "tmpfs",
        protect_system: None,
        agent_access: false,
        private_network: None,
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
        protect_system: None,
        agent_access: false,
        private_network: None,
        rw_paths: &[],
        ro_paths: &[],
    },
    Profile {
        name: "coding-agent",
        description: "AI coding agents with shared agent state and restricted host access",
        memory_limit: Some("4G"),
        cpu_quota: Some("200%"),
        memory_swap_max: Some("0"),
        protect_home: "tmpfs",
        protect_system: Some("strict"),
        agent_access: true,
        private_network: None,
        rw_paths: &[
            "$HOME/.claude",
            "$HOME/.codex",
            "$HOME/.gemini",
            "$HOME/.pi/agent",
            "$HOME/.pi/context-mode",
            "$HOME/.pi/web-search-cache",
            "$HOME/.pi/web-search.json",
            "$HOME/.cargo/registry",
            "$HOME/.cargo/git",
        ],
        ro_paths: &[
            "$HOME/.agents",
            "$HOME/.cargo/bin",
            "$HOME/.rustup",
            "$HOME/.local/share/claude",
        ],
    },
    Profile {
        name: "shell",
        description: "Interactive shell/terminal session (read-only home)",
        memory_limit: Some("4G"),
        cpu_quota: None,
        memory_swap_max: Some("0"),
        protect_home: "read-only",
        protect_system: None,
        agent_access: false,
        private_network: None,
        rw_paths: &["$HOME/.local/share", "$HOME/.cache", "$HOME/.local/bin"],
        ro_paths: &[],
    },
];

/// Look up a profile by name, printing the list of valid profiles and
/// exiting if the name is not recognized.
fn lookup_profile(name: &str) -> &'static Profile {
    PROFILES.iter().find(|p| p.name == name).unwrap_or_else(|| {
        eprintln!("error: unknown profile '{}'\n", name);
        eprintln!("Valid profiles:");
        for p in PROFILES {
            eprintln!("  {:12} - {}", p.name, p.description);
        }
        std::process::exit(1);
    })
}

/// Expand `$HOME`, `$UID` and similar variables in a profile path.
fn expand_path(path: &str) -> String {
    shellexpand::env(path)
        .unwrap_or_else(|_| path.into())
        .into_owned()
}

/// Add a profile path to `list`, but only if it currently exists. systemd-run
/// refuses to start if asked to bind-mount a missing path, so a profile that
/// names, say, `$HOME/.cargo` on a machine without Cargo simply skips it.
fn push_if_exists(list: &mut Vec<String>, raw: &str) {
    let expanded = expand_path(raw);
    if std::path::Path::new(&expanded).exists() {
        list.push(expanded);
    }
}

/// Refuse automatic mounts whose symlinks could expose an unrelated tree.
/// Explicit mounts are a deliberate escape hatch; do not second-guess them.
fn push_agent_path(list: &mut Vec<String>, raw: &str, explicit: &[String]) -> Result<()> {
    push_agent_mount(list, Path::new(&expand_path(raw)), explicit)
}

fn push_agent_mount(list: &mut Vec<String>, path: &Path, explicit: &[String]) -> Result<()> {
    if explicit.iter().any(|p| Path::new(p) == path) {
        return Ok(());
    }
    match path.canonicalize() {
        Ok(target) => {
            if target != path {
                bail!(
                    "automatic coding-agent mount {} resolves to {}; review it and use an explicit --rw/--ro mount",
                    path.display(), target.display()
                );
            }
            list.push(path.to_string_lossy().into_owned());
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e).with_context(|| format!("inspect agent path {}", path.display())),
    }
    Ok(())
}

/// The cwd is always writable, so a broad or sensitive cwd defeats hidden home.
fn validate_agent_workspace() -> Result<()> {
    let home = PathBuf::from(std::env::var_os("HOME").context("coding-agent requires HOME")?)
        .canonicalize()
        .context("resolve HOME for coding-agent")?;
    let cwd = std::env::current_dir().context("resolve coding-agent working directory")?;
    let sensitive = [
        ".ssh", ".aws", ".kube", ".docker", ".config", ".gnupg", ".local", ".cache", ".cargo",
        ".rustup", ".pi", ".claude", ".codex", ".gemini", ".agents",
    ];
    if home.starts_with(&cwd)
        || sensitive.iter().any(|name| {
            let path = home.join(name);
            cwd.starts_with(&path)
                || path
                    .canonicalize()
                    .is_ok_and(|target| cwd.starts_with(target))
        })
        || ["/etc", "/run", "/proc", "/sys", "/dev"]
            .iter()
            .any(|p| cwd.starts_with(p))
    {
        bail!("coding-agent must run from a project directory, not home, an ancestor, or a configuration/credential directory");
    }
    Ok(())
}

/// Resolve version-manager PATH aliases without exposing /run/user sockets.
fn agent_search_path() -> Result<String> {
    let search = std::env::var_os("PATH").unwrap_or_default();
    let mut paths = Vec::new();
    for path in std::env::split_paths(&search) {
        let path = path.canonicalize().unwrap_or(path);
        if path.is_absolute() && !paths.contains(&path) {
            paths.push(path);
        }
    }
    std::env::join_paths(paths)
        .context("build coding-agent PATH")?
        .into_string()
        .map_err(|_| anyhow::anyhow!("coding-agent PATH must be UTF-8"))
}

/// Expose selected executable files, not all of ~/.local/bin or a version
/// manager's data. npm CLIs need both bin symlinks and node_modules intact.
fn discover_agent_tools(paths: &mut Vec<String>) -> Result<()> {
    let search = agent_search_path()?;
    for name in ["claude", "codex", "gemini", "pi", "node", "uv", "rtk", "gh"] {
        let Some(executable) = std::env::split_paths(&search)
            .filter(|p| p.is_absolute())
            .map(|p| p.join(name))
            .find(|p| {
                p.metadata()
                    .is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
            })
        else {
            continue;
        };
        let target = executable
            .canonicalize()
            .with_context(|| format!("resolve {}", executable.display()))?;
        let bin = executable.parent().context("find executable directory")?;
        let modules = target
            .ancestors()
            .find(|p| p.file_name().is_some_and(|n| n == "node_modules"));
        let npm_bin = modules.is_some()
            && bin.file_name().is_some_and(|n| n == "bin")
            && bin
                .parent()
                .is_some_and(|p| p.join("lib/node_modules").is_dir());
        // Binding an npm symlink as an individual file flattens the link,
        // breaking imports relative to the original JS entry point.
        let entry = if npm_bin { bin } else { executable.as_path() };
        for path in [Some(entry), Some(target.as_path()), modules]
            .into_iter()
            .flatten()
        {
            let value = path.to_string_lossy().into_owned();
            if !paths.contains(&value) {
                paths.push(value);
            }
        }
    }
    Ok(())
}

/// Follow gh's Linux config precedence without mounting all of XDG_CONFIG_HOME.
/// Inspect paths only; credential file contents never pass through playpen.
fn github_config_dir() -> Result<PathBuf> {
    let nonempty = |name| std::env::var_os(name).filter(|v| !v.is_empty());
    let home = PathBuf::from(std::env::var_os("HOME").context("coding-agent requires HOME")?);
    let path = if let Some(dir) = nonempty("GH_CONFIG_DIR") {
        PathBuf::from(dir)
    } else if let Some(dir) = nonempty("XDG_CONFIG_HOME") {
        PathBuf::from(dir).join("gh")
    } else {
        home.join(".config/gh")
    };
    if !path.is_absolute()
        || path
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
        || home.starts_with(&path)
        || path == home.join(".config")
        || path == home.join(".local")
        || path == home.join(".local/share")
        || [".ssh", ".aws", ".kube", ".docker", ".gnupg"]
            .iter()
            .any(|p| path.starts_with(home.join(p)))
        || path.starts_with("/run")
    {
        bail!("GH_CONFIG_DIR must name a dedicated absolute GitHub CLI configuration directory, not a broad or credential directory");
    }
    if path.exists() && !path.is_dir() {
        bail!(
            "GitHub CLI configuration path is not a directory: {}",
            path.display()
        );
    }
    if path
        .canonicalize()
        .is_ok_and(|p| std::env::current_dir().is_ok_and(|cwd| cwd.starts_with(p)))
    {
        bail!("coding-agent must run from a project directory, not the GitHub CLI configuration directory");
    }
    Ok(path)
}

/// env -i is necessary: not capturing the shell environment does not remove
/// secrets already inherited from the systemd user manager.
fn agent_environment(
    cli: &Run,
    gh_config: &Path,
    forward_github_token: bool,
) -> Result<Vec<String>> {
    let mut env = std::collections::BTreeMap::new();
    if cli.capture_env {
        for (key, value) in std::env::vars() {
            if key != "DBUS_SESSION_BUS_ADDRESS"
                && key != "GH_TOKEN"
                && !(key.starts_with("BASH_FUNC_") && key.ends_with("%%"))
            {
                env.insert(key, value);
            }
        }
    } else {
        for key in [
            "HOME",
            "USER",
            "LOGNAME",
            "TERM",
            "COLORTERM",
            "LANG",
            "LC_ALL",
            "LC_CTYPE",
            "TZ",
        ] {
            if let Ok(value) = std::env::var(key) {
                env.insert(key.to_string(), value);
            }
        }
        env.insert("SHELL".to_string(), "/bin/sh".to_string());
        env.insert("TMPDIR".to_string(), "/tmp".to_string());
        env.insert(
            "XDG_CACHE_HOME".to_string(),
            "/tmp/playpen-cache".to_string(),
        );
        env.insert(
            "npm_config_cache".to_string(),
            "/tmp/playpen-cache/npm".to_string(),
        );
        env.insert(
            "UV_CACHE_DIR".to_string(),
            "/tmp/playpen-cache/uv".to_string(),
        );
    }
    let path = if cli.capture_env || cli.capture_path {
        agent_search_path()?
    } else {
        "/usr/local/bin:/usr/bin:/bin".to_string()
    };
    env.insert("PATH".to_string(), path);
    env.insert(
        "GH_CONFIG_DIR".to_string(),
        gh_config
            .to_str()
            .context("GitHub CLI configuration path must be UTF-8")?
            .to_string(),
    );
    // Claude's legacy ~/.claude.json needs sibling lock files and atomic
    // replacement. Use the mounted directory instead; never copy credentials.
    let home = std::env::var("HOME").context("read HOME for coding-agent")?;
    env.entry("CLAUDE_CONFIG_DIR".to_string())
        .or_insert_with(|| format!("{home}/.claude"));
    let mut prefix = vec!["/usr/bin/env".to_string(), "-i".to_string()];
    if forward_github_token {
        // GNU env expands this from the service environment without exposing
        // the token in ExecStart, process arguments, or --dry-run output.
        // Expansion is literal: spaces/metacharacters in values aren't parsed.
        prefix.extend(["-S".to_string(), "-- GH_TOKEN=${GH_TOKEN}".to_string()]);
    } else {
        prefix.push("--".to_string());
    }
    prefix.extend(env.into_iter().map(|(k, v)| format!("{k}={v}")));
    Ok(prefix)
}

// ============ CLI ============

const PROFILE_HELP: &str = "Use a predefined resource and filesystem profile. Valid profiles: cargo, npm, pytest, python, uv, go, make, coding-agent, shell";

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Run {
    #[clap(short, long)]
    memory_limit: Option<String>,

    #[clap(short, long)]
    cpu_limit: Option<String>,

    // Disk I/O bandwidth limits. `--disk-limit` caps the read and write
    // rates together; `--disk-read`/`--disk-write` cap a single direction and
    // override `--disk-limit` for that direction. Values are byte/sec rates
    // for the block device backing the working directory, using systemd's
    // K/M/G/T suffixes (e.g. 50M, 500K).
    #[clap(
        short,
        long,
        value_name = "VALUE",
        help = "Limit disk I/O bandwidth, read and write, for the working directory's device (e.g. 50M, 500K)"
    )]
    disk_limit: Option<String>,

    #[clap(
        long,
        value_name = "VALUE",
        help = "Limit disk read bandwidth only; overrides --disk-limit for reads"
    )]
    disk_read: Option<String>,

    #[clap(
        long,
        value_name = "VALUE",
        help = "Limit disk write bandwidth only; overrides --disk-limit for writes"
    )]
    disk_write: Option<String>,

    #[clap(short, long, default_value = "false")]
    quiet: bool,

    #[arg(long, action = ArgAction::Set, value_parser = BoolishValueParser::new(), default_value = "false")]
    capture_env: bool,

    #[arg(long, action = ArgAction::Set, value_parser = BoolishValueParser::new(), default_value = "true")]
    capture_path: bool,

    #[clap(long, value_name = "NAME", help = PROFILE_HELP)]
    profile: Option<String>,

    #[clap(
        long,
        value_name = "VALUE",
        help = "Set MemorySwapMax limit (e.g., 0, 1G)"
    )]
    memory_swap_max: Option<String>,

    #[clap(
        long,
        help = "Print the resolved systemd-run command without executing"
    )]
    dry_run: bool,

    // Fine-grained path controls
    #[arg(long = "rw", help = "Add read-write path access (can be repeated)")]
    rw_paths: Vec<String>,

    #[arg(long = "ro", help = "Add read-only path access (can be repeated)")]
    ro_paths: Vec<String>,

    #[arg(long, help = "Make path completely inaccessible (can be repeated)")]
    inaccessible: Vec<String>,

    // Protection flags. Unset means "use the default (on)"; pass an explicit
    // value to override a profile or to turn the protection off.
    #[arg(long, value_parser = BoolishValueParser::new(), help = "Use private /tmp (default: true)")]
    private_tmp: Option<bool>,

    #[arg(long, value_parser = BoolishValueParser::new(), help = "Use private /dev (default: true)")]
    private_devices: Option<bool>,

    #[arg(long, value_parser = BoolishValueParser::new(), help = "Protect kernel tunables (default: true)")]
    protect_kernel_tunables: Option<bool>,

    #[arg(long, value_parser = BoolishValueParser::new(), help = "Protect control groups (default: true)")]
    protect_control_groups: Option<bool>,

    #[arg(long, help = "Protect home directories: none/yes/read-only/tmpfs")]
    protect_home: Option<String>,

    #[arg(long, help = "Protect system directories: none/yes/full/strict")]
    protect_system: Option<String>,

    // Network controls.
    #[arg(
        long,
        value_parser = BoolishValueParser::new(),
        help = "Use a private network namespace, no external network (default: off)"
    )]
    private_network: Option<bool>,

    #[arg(
        long = "ip-allow",
        help = "Allow IP/CIDR for network traffic (can be repeated)"
    )]
    ip_allow: Vec<String>,

    #[arg(
        long = "ip-deny",
        help = "Deny IP/CIDR for network traffic (can be repeated)"
    )]
    ip_deny: Vec<String>,

    #[arg(
        long = "socket-bind-allow",
        help = "Allow bind() rule for listening sockets (can be repeated)"
    )]
    socket_bind_allow: Vec<String>,

    #[arg(
        long = "socket-bind-deny",
        help = "Deny bind() rule for listening sockets (can be repeated)"
    )]
    socket_bind_deny: Vec<String>,

    #[arg(long, help = "Restrictive preset: only current directory accessible")]
    current_dir_only: bool,

    #[clap()]
    command_and_args: Vec<String>,
}

// ============ Resolved Configuration ============

/// The sandbox settings after a profile, the `--current-dir-only` preset and
/// any explicit flags have all been merged. This is the single source of
/// truth that `to_systemd_args` renders; nothing downstream re-reads the CLI.
struct Config {
    memory_max: Option<String>,
    memory_swap_max: Option<String>,
    cpu_quota: Option<String>,
    /// Per-direction disk I/O bandwidth ceilings (bytes/sec) for the block
    /// device backing the working directory. `None` leaves that direction
    /// unlimited.
    io_read_bandwidth_max: Option<String>,
    io_write_bandwidth_max: Option<String>,
    /// systemd `ProtectHome` value (`yes`/`read-only`/`tmpfs`); `None` leaves
    /// the home directory unrestricted.
    protect_home: Option<String>,
    protect_system: Option<String>,
    /// An env -i command prefix for the coding-agent profile; empty otherwise.
    command_prefix: Vec<String>,
    /// Forward only the name to systemd-run; never put its secret value in argv.
    forward_github_token: bool,
    private_tmp: bool,
    private_devices: bool,
    protect_kernel_tunables: bool,
    protect_control_groups: bool,
    bind_paths: Vec<String>,
    bind_ro_paths: Vec<String>,
    inaccessible_paths: Vec<String>,
    /// Bind-mount the current directory read-write. Needed whenever the home
    /// directory is hidden, so the project being worked on stays reachable.
    bind_cwd: bool,
    /// Network namespace isolation. `Some(true)` emits `PrivateNetwork=yes`,
    /// `Some(false)` emits `PrivateNetwork=no`, `None` emits nothing.
    private_network: Option<bool>,
    /// `IPAddressAllow=` / `IPAddressDeny=` entries, in CLI order.
    ip_allow: Vec<String>,
    ip_deny: Vec<String>,
    /// `SocketBindAllow=` / `SocketBindDeny=` rules, in CLI order.
    socket_bind_allow: Vec<String>,
    socket_bind_deny: Vec<String>,
}

impl Config {
    /// Merge CLI arguments into the final sandbox configuration.
    ///
    /// Precedence, lowest to highest: built-in defaults, then `--profile`,
    /// then `--current-dir-only`, then explicit per-setting flags. Command-line
    /// order is irrelevant — an explicit flag always beats the profile. Path
    /// flags (`--rw`/`--ro`/`--inaccessible`) accumulate rather than override.
    fn resolve(cli: &Run) -> Result<Config> {
        // Defaults: the four namespace protections are on; nothing else set.
        let mut c = Config {
            memory_max: None,
            memory_swap_max: None,
            cpu_quota: None,
            io_read_bandwidth_max: None,
            io_write_bandwidth_max: None,
            protect_home: None,
            protect_system: None,
            command_prefix: Vec::new(),
            forward_github_token: false,
            private_tmp: true,
            private_devices: true,
            protect_kernel_tunables: true,
            protect_control_groups: true,
            bind_paths: Vec::new(),
            bind_ro_paths: Vec::new(),
            inaccessible_paths: Vec::new(),
            bind_cwd: false,
            private_network: None,
            ip_allow: Vec::new(),
            ip_deny: Vec::new(),
            socket_bind_allow: Vec::new(),
            socket_bind_deny: Vec::new(),
        };

        // Profile baseline.
        let profile = cli.profile.as_deref().map(lookup_profile);
        if let Some(p) = profile {
            c.memory_max = p.memory_limit.map(String::from);
            c.cpu_quota = p.cpu_quota.map(String::from);
            c.memory_swap_max = p.memory_swap_max.map(String::from);
            c.protect_home = Some(p.protect_home.to_string());
            c.protect_system = p.protect_system.map(String::from);
            c.private_network = p.private_network;
            c.bind_cwd = true;
            if p.agent_access {
                // Otherwise env would print its environment, including GH_TOKEN.
                if cli.command_and_args.is_empty() && !cli.dry_run {
                    bail!("coding-agent requires a command after --");
                }
                validate_agent_workspace()?;
                let gh_config = github_config_dir()?;
                c.forward_github_token =
                    std::env::var_os("GH_TOKEN").is_some_and(|v| !v.is_empty());
                c.command_prefix = agent_environment(cli, &gh_config, c.forward_github_token)?;
                push_agent_mount(&mut c.bind_ro_paths, &gh_config, &cli.ro_paths)?;
                for path in p.rw_paths {
                    push_agent_path(&mut c.bind_paths, path, &cli.rw_paths)?;
                }
                for path in p.ro_paths {
                    push_agent_path(&mut c.bind_ro_paths, path, &cli.ro_paths)?;
                }
                discover_agent_tools(&mut c.bind_ro_paths)?;
                // '-' tolerates absent sockets without a check/use race.
                c.inaccessible_paths
                    .extend(["-/run/docker.sock".to_string(), "-/run/podman".to_string()]);
            } else {
                for path in p.rw_paths {
                    push_if_exists(&mut c.bind_paths, path);
                }
                for path in p.ro_paths {
                    push_if_exists(&mut c.bind_ro_paths, path);
                }
            }
        }

        // The --current-dir-only preset: hide home, keep only the cwd. This is
        // deliberately a *filesystem*-only preset — it does not touch network
        // settings, so a user who picks it for filesystem reasons is not
        // surprised by network failures. Compose with --private-network for
        // both.
        if cli.current_dir_only {
            c.protect_home = Some("tmpfs".to_string());
            c.bind_cwd = true;
        }

        // Explicit flags override the profile and preset above.
        if let Some(v) = &cli.memory_limit {
            c.memory_max = Some(v.clone());
        }
        if let Some(v) = &cli.cpu_limit {
            c.cpu_quota = Some(v.clone());
        }
        // Disk I/O: --disk-limit seeds both directions; a direction-specific
        // flag then overrides its own side.
        if let Some(v) = &cli.disk_limit {
            c.io_read_bandwidth_max = Some(v.clone());
            c.io_write_bandwidth_max = Some(v.clone());
        }
        if let Some(v) = &cli.disk_read {
            c.io_read_bandwidth_max = Some(v.clone());
        }
        if let Some(v) = &cli.disk_write {
            c.io_write_bandwidth_max = Some(v.clone());
        }
        if let Some(v) = &cli.memory_swap_max {
            c.memory_swap_max = Some(v.clone());
        }
        if let Some(v) = &cli.protect_home {
            c.protect_home = normalize_protect(v);
        }
        if let Some(v) = &cli.protect_system {
            c.protect_system = normalize_protect(v);
        }
        if let Some(v) = cli.private_tmp {
            c.private_tmp = v;
        }
        if let Some(v) = cli.private_devices {
            c.private_devices = v;
        }
        if let Some(v) = cli.protect_kernel_tunables {
            c.protect_kernel_tunables = v;
        }
        if let Some(v) = cli.protect_control_groups {
            c.protect_control_groups = v;
        }
        if let Some(v) = cli.private_network {
            c.private_network = Some(v);
        }

        // Path flags accumulate on top of any profile paths.
        c.bind_paths.extend(cli.rw_paths.iter().cloned());
        c.bind_ro_paths.extend(cli.ro_paths.iter().cloned());
        c.inaccessible_paths
            .extend(cli.inaccessible.iter().cloned());
        c.ip_allow.extend(cli.ip_allow.iter().cloned());
        c.ip_deny.extend(cli.ip_deny.iter().cloned());
        c.socket_bind_allow
            .extend(cli.socket_bind_allow.iter().cloned());
        c.socket_bind_deny
            .extend(cli.socket_bind_deny.iter().cloned());

        // A bare memory limit gets a hard ceiling by disabling swap. Profiles
        // pick their own swap policy, so this default applies only when no
        // profile is active.
        if c.memory_max.is_some() && c.memory_swap_max.is_none() && profile.is_none() {
            c.memory_swap_max = Some("0".to_string());
        }

        Ok(c)
    }

    /// Render the configuration as `systemd-run` `-p` property arguments.
    fn to_systemd_args(&self) -> Vec<String> {
        let mut args = Vec::new();

        if let Some(v) = &self.memory_max {
            args.push(format!("-pMemoryMax={}", v));
        }
        if let Some(v) = &self.memory_swap_max {
            args.push(format!("-pMemorySwapMax={}", v));
        }
        if let Some(v) = &self.cpu_quota {
            args.push(format!("-pCPUQuota={}", v));
            args.push(format!("-pCPUQuotaPeriodSec={}", DEFAULT_CPU_QUOTA_PERIOD));
        }
        // Disk I/O limits name the block device backing the working
        // directory — where a sandboxed build does its real I/O. systemd
        // resolves the path to its device, so passing the path is enough.
        if self.io_read_bandwidth_max.is_some() || self.io_write_bandwidth_max.is_some() {
            if let Ok(pwd) = std::env::current_dir() {
                if let Some(v) = &self.io_read_bandwidth_max {
                    args.push(format!(
                        "-pIOReadBandwidthMax={} {}",
                        unit_path(&pwd.to_string_lossy()),
                        v
                    ));
                }
                if let Some(v) = &self.io_write_bandwidth_max {
                    args.push(format!(
                        "-pIOWriteBandwidthMax={} {}",
                        unit_path(&pwd.to_string_lossy()),
                        v
                    ));
                }
            }
        }
        if self.private_tmp {
            args.push("-pPrivateTmp=yes".to_string());
        }
        if self.private_devices {
            args.push("-pPrivateDevices=yes".to_string());
        }
        if self.protect_kernel_tunables {
            args.push("-pProtectKernelTunables=yes".to_string());
        }
        if self.protect_control_groups {
            args.push("-pProtectControlGroups=yes".to_string());
        }
        if let Some(v) = &self.protect_home {
            args.push(format!("-pProtectHome={}", v));
        }
        if let Some(v) = &self.protect_system {
            args.push(format!("-pProtectSystem={}", v));
        }

        if let Some(v) = self.private_network {
            args.push(format!("-pPrivateNetwork={}", if v { "yes" } else { "no" }));
        }
        for v in &self.ip_allow {
            args.push(format!("-pIPAddressAllow={}", v));
        }
        for v in &self.ip_deny {
            args.push(format!("-pIPAddressDeny={}", v));
        }
        for v in &self.socket_bind_allow {
            args.push(format!("-pSocketBindAllow={}", v));
        }
        for v in &self.socket_bind_deny {
            args.push(format!("-pSocketBindDeny={}", v));
        }

        if self.bind_cwd {
            if let Ok(pwd) = std::env::current_dir() {
                args.push(format!("-pBindPaths={}", unit_path(&pwd.to_string_lossy())));
            }
        }
        for p in &self.bind_paths {
            args.push(format!("-pBindPaths={}", unit_path(p)));
        }
        for p in &self.bind_ro_paths {
            args.push(format!("-pBindReadOnlyPaths={}", unit_path(p)));
        }
        for p in &self.inaccessible_paths {
            args.push(format!("-pInaccessiblePaths={}", unit_path(p)));
        }

        args
    }
}

/// Quote one path for systemd-run's property parser, not the invoking shell.
/// Transient D-Bus path properties take literal '%' (unlike unit files).
fn unit_path(path: &str) -> String {
    let escaped = path
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t");
    if path
        .chars()
        .any(|c| c.is_whitespace() || matches!(c, '\\' | '"' | '\'' | ':'))
    {
        format!("\"{escaped}\"")
    } else {
        escaped
    }
}

/// Translate a `--protect-home`/`--protect-system` value into an emittable
/// setting: the sentinel `none` means "do not restrict" (`None`).
fn normalize_protect(value: &str) -> Option<String> {
    if value == "none" {
        None
    } else {
        Some(value.to_string())
    }
}

/// Quote an argument for safe display in `--dry-run` output. Anything outside
/// a conservative set of shell-safe characters is single-quoted.
fn shell_quote(s: &str) -> String {
    let safe = !s.is_empty()
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_-./=:%+,".contains(&b));
    if safe {
        s.to_string()
    } else {
        format!("'{}'", s.replace('\'', "'\\''"))
    }
}

// ============ Main ============

fn main() -> Result<()> {
    let cli = Run::parse();
    let config = Config::resolve(&cli)?;

    let mut parts = vec!["systemd-run".to_string()];
    let base_command = "--user --same-dir --wait --pipe";
    parts.extend(base_command.split_whitespace().map(String::from));

    // Include all env vars in the calling environment
    if !config.command_prefix.is_empty() {
        // The coding-agent profile clears even the user manager's environment.
        // Prevent systemd-run from expanding '$' in the env assignments/command.
        parts.push("--expand-environment=no".to_string());
        if config.forward_github_token {
            parts.push("--setenv=GH_TOKEN".to_string());
        }
    } else if cli.capture_env {
        for (key, value) in std::env::vars() {
            // Skip the environment variables that systemd-run sets
            if key == "DBUS_SESSION_BUS_ADDRESS" {
                continue;
            }
            // Skip env vars that are actually exported bash functions
            if key.starts_with("BASH_FUNC_") && key.ends_with("%%") {
                continue;
            }
            parts.push(format!(r#"--setenv={}="{}""#, key, value));
        }
    } else if cli.capture_path {
        if let Some(path) = std::env::var_os("PATH") {
            parts.push(format!(r#"--setenv=PATH="{}""#, path.to_string_lossy()));
        }
    }

    // Only add --pty if we are attached to a terminal
    if atty::is(Stream::Stdout) && atty::is(Stream::Stdin) {
        parts.push("--pty".to_string());
    }

    if cli.quiet {
        parts.push("--quiet".to_string());
    }

    parts.extend(config.to_systemd_args());
    parts.extend(config.command_prefix);
    parts.extend(cli.command_and_args.clone());

    if cli.dry_run {
        let rendered: Vec<String> = parts.iter().map(|s| shell_quote(s)).collect();
        println!("{}", rendered.join(" "));
        return Ok(());
    }

    let execvp_args: Vec<CString> = parts
        .iter()
        .map(|s| CString::new(s.clone()).unwrap())
        .collect();

    execvp(&execvp_args[0], &execvp_args)?;

    Ok(())
}
