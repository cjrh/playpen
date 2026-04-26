use anyhow::Result;
use atty::Stream;
use clap::builder::BoolishValueParser;
use clap::ArgAction;
use clap::Parser;
use nix::unistd::execvp;
use std::collections::HashMap;
use std::ffi::CString;

// ============ Profile Definitions ============

#[derive(Debug, Clone, Copy)]
struct Profile {
    name: &'static str,
    description: &'static str,
    memory_limit: Option<&'static str>,
    cpu_quota: Option<&'static str>,
    memory_swap_max: Option<&'static str>,
    protect_home: &'static str,
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
        rw_paths: &[],
        ro_paths: &["$HOME/.gitconfig", "$HOME/.ssh"],
    },
    Profile {
        name: "shell",
        description: "Interactive shell/terminal session (read-only home)",
        memory_limit: Some("4G"),
        cpu_quota: None,
        memory_swap_max: Some("0"),
        protect_home: "read-only",
        rw_paths: &["$HOME/.local/share", "$HOME/.cache", "$HOME/.local/bin"],
        ro_paths: &[],
    },
];

fn find_profile(name: &str) -> Option<&'static Profile> {
    PROFILES.iter().find(|p| p.name == name)
}

fn expand_path(path: &str) -> String {
    shellexpand::env(path)
        .unwrap_or_else(|_| path.into())
        .into_owned()
}

// ============ SystemdProps Builder ============

#[derive(Debug)]
struct SystemdProps {
    props: HashMap<String, String>,
    lists: HashMap<String, Vec<String>>,
    lockdown_base: bool,
    profile_applied: bool,
}

impl SystemdProps {
    fn new() -> Self {
        Self {
            props: HashMap::new(),
            lists: HashMap::new(),
            lockdown_base: false,
            profile_applied: false,
        }
    }

    fn set(&mut self, key: &str, value: &str) {
        self.props.insert(key.to_string(), value.to_string());
    }

    fn get(&self, key: &str) -> Option<&String> {
        self.props.get(key)
    }

    fn push(&mut self, key: &str, value: String) {
        self.lists.entry(key.to_string()).or_default().push(value);
    }

    fn apply_lockdown_base(&mut self, protect_home: &str) {
        self.set("ProtectHome", protect_home);
        self.set("PrivateTmp", "yes");
        self.set("PrivateDevices", "yes");
        self.set("ProtectKernelTunables", "yes");
        self.set("ProtectControlGroups", "yes");
        self.lockdown_base = true;
    }

    fn emit(self) -> Vec<String> {
        let mut result = Vec::new();

        // MemoryMax
        if let Some(v) = self.props.get("MemoryMax") {
            result.push(format!("-pMemoryMax={}", v));
        }

        // MemorySwapMax
        if let Some(v) = self.props.get("MemorySwapMax") {
            result.push(format!("-pMemorySwapMax={}", v));
        }

        // CPUQuota + CPUQuotaPeriodSec
        if let Some(v) = self.props.get("CPUQuota") {
            result.push(format!("-pCPUQuota={}", v));
            result.push(format!("-pCPUQuotaPeriodSec={}", DEFAULT_CPU_QUOTA_PERIOD));
        }

        // Lockdown base (from profile or --current-dir-only)
        if self.lockdown_base {
            let protect_home = self
                .props
                .get("ProtectHome")
                .map(|s| s.as_str())
                .unwrap_or("no");
            if protect_home != "no" {
                for key in [
                    "PrivateTmp",
                    "PrivateDevices",
                    "ProtectKernelTunables",
                    "ProtectControlGroups",
                ] {
                    if let Some(v) = self.props.get(key) {
                        result.push(format!("-p{}={}", key, v));
                    }
                }
                result.push(format!("-pProtectHome={}", protect_home));
                if let Ok(pwd) = std::env::current_dir() {
                    result.push(format!("-pBindPaths={}", pwd.display()));
                }
            }
        } else {
            // Individual protection flags
            for key in [
                "PrivateTmp",
                "PrivateDevices",
                "ProtectKernelTunables",
                "ProtectControlGroups",
            ] {
                if let Some(v) = self.props.get(key) {
                    result.push(format!("-p{}={}", key, v));
                }
            }
            if let Some(v) = self.props.get("ProtectHome") {
                result.push(format!("-pProtectHome={}", v));
            }
            if let Some(v) = self.props.get("ProtectSystem") {
                result.push(format!("-pProtectSystem={}", v));
            }
        }

        // List properties (accumulated from profile and explicit flags)
        for (key, paths) in self.lists {
            for path in paths {
                result.push(format!("-p{}={}", key, path));
            }
        }

        result
    }
}

// ============ CLI ============

const PROFILE_HELP: &str = "Use a predefined resource and filesystem profile. Valid profiles: cargo, npm, pytest, python, uv, go, make, coding_agent, shell";

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Run {
    #[clap(short, long)]
    memory_limit: Option<String>,

    #[clap(short, long)]
    cpu_limit: Option<String>,

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

    // Protection flags with defaults
    #[arg(long, action = ArgAction::Set, value_parser = BoolishValueParser::new(), default_value = "true", help = "Use private /tmp")]
    private_tmp: bool,

    #[arg(long, action = ArgAction::Set, value_parser = BoolishValueParser::new(), default_value = "true", help = "Use private /dev")]
    private_devices: bool,

    #[arg(long, action = ArgAction::Set, value_parser = BoolishValueParser::new(), default_value = "true", help = "Protect kernel tunables")]
    protect_kernel_tunables: bool,

    #[arg(long, action = ArgAction::Set, value_parser = BoolishValueParser::new(), default_value = "true", help = "Protect control groups")]
    protect_control_groups: bool,

    #[arg(
        long,
        default_value = "none",
        help = "Protect home directories: none/yes/read-only/tmpfs"
    )]
    protect_home: String,

    #[arg(
        long,
        default_value = "none",
        help = "Protect system directories: none/yes/full/strict"
    )]
    protect_system: String,

    // Preset configurations
    #[arg(
        long,
        default_value = "false",
        help = "Restrictive preset: only current directory accessible"
    )]
    current_dir_only: bool,

    #[clap()]
    command_and_args: Vec<String>,
}

// ============ Argument Processing ============

fn try_split_equals<'a>(arg: &'a str, prefix: &str) -> Option<&'a str> {
    if arg.starts_with(prefix) && arg.as_bytes().get(prefix.len()) == Some(&b'=') {
        Some(&arg[prefix.len() + 1..])
    } else {
        None
    }
}

fn apply_profile(props: &mut SystemdProps, profile: &Profile) {
    props.profile_applied = true;

    if let Some(mem) = profile.memory_limit {
        props.set("MemoryMax", mem);
    }
    if let Some(cpu) = profile.cpu_quota {
        props.set("CPUQuota", cpu);
    }
    if let Some(swap) = profile.memory_swap_max {
        props.set("MemorySwapMax", swap);
    }

    if profile.protect_home != "no" {
        props.apply_lockdown_base(profile.protect_home);
    }

    for path in profile.rw_paths {
        let expanded = expand_path(path);
        if std::path::Path::new(&expanded).exists() {
            props.push("BindPaths", expanded);
        }
    }

    for path in profile.ro_paths {
        let expanded = expand_path(path);
        if std::path::Path::new(&expanded).exists() {
            props.push("BindReadOnlyPaths", expanded);
        }
    }
}

fn apply_profile_by_name(props: &mut SystemdProps, name: &str) {
    if let Some(profile) = find_profile(name) {
        apply_profile(props, profile);
    } else {
        eprintln!("error: unknown profile '{}'", name);
        eprintln!();
        eprintln!("Valid profiles:");
        for p in PROFILES {
            eprintln!("  {:12} - {}", p.name, p.description);
        }
        std::process::exit(1);
    }
}

#[derive(Default)]
struct ExplicitFlags {
    private_tmp: Option<bool>,
    private_devices: Option<bool>,
    protect_kernel_tunables: Option<bool>,
    protect_control_groups: Option<bool>,
    protect_home: Option<String>,
    protect_system: Option<String>,
}

fn parse_boolish(s: &str) -> Option<bool> {
    let s = s.to_lowercase();
    if s == "true" || s == "yes" || s == "1" || s == "on" {
        Some(true)
    } else if s == "false" || s == "no" || s == "0" || s == "off" {
        Some(false)
    } else {
        None
    }
}

fn process_ordered_args(props: &mut SystemdProps, explicit: &mut ExplicitFlags) {
    let args: Vec<String> = std::env::args().collect();
    let mut i = 1; // skip program name

    while i < args.len() {
        let arg = &args[i];

        // Stop at -- (command separator)
        if arg == "--" {
            break;
        }

        // Try --flag=value format first
        if let Some(value) = try_split_equals(arg, "--profile") {
            apply_profile_by_name(props, value);
            i += 1;
            continue;
        }
        if let Some(value) = try_split_equals(arg, "-m") {
            props.set("MemoryMax", value);
            i += 1;
            continue;
        }
        if let Some(value) = try_split_equals(arg, "--memory-limit") {
            props.set("MemoryMax", value);
            i += 1;
            continue;
        }
        if let Some(value) = try_split_equals(arg, "-c") {
            props.set("CPUQuota", value);
            i += 1;
            continue;
        }
        if let Some(value) = try_split_equals(arg, "--cpu-limit") {
            props.set("CPUQuota", value);
            i += 1;
            continue;
        }
        if let Some(value) = try_split_equals(arg, "--memory-swap-max") {
            props.set("MemorySwapMax", value);
            i += 1;
            continue;
        }
        if let Some(value) = try_split_equals(arg, "--protect-home") {
            let normalized = if value == "none" {
                "no".to_string()
            } else {
                value.to_string()
            };
            props.set("ProtectHome", &normalized);
            explicit.protect_home = Some(normalized);
            i += 1;
            continue;
        }
        if let Some(value) = try_split_equals(arg, "--protect-system") {
            props.set("ProtectSystem", value);
            explicit.protect_system = Some(value.to_string());
            i += 1;
            continue;
        }
        if let Some(value) = try_split_equals(arg, "--private-tmp") {
            if let Some(b) = parse_boolish(value) {
                props.set("PrivateTmp", if b { "yes" } else { "no" });
                explicit.private_tmp = Some(b);
            }
            i += 1;
            continue;
        }
        if let Some(value) = try_split_equals(arg, "--private-devices") {
            if let Some(b) = parse_boolish(value) {
                props.set("PrivateDevices", if b { "yes" } else { "no" });
                explicit.private_devices = Some(b);
            }
            i += 1;
            continue;
        }
        if let Some(value) = try_split_equals(arg, "--protect-kernel-tunables") {
            if let Some(b) = parse_boolish(value) {
                props.set("ProtectKernelTunables", if b { "yes" } else { "no" });
                explicit.protect_kernel_tunables = Some(b);
            }
            i += 1;
            continue;
        }
        if let Some(value) = try_split_equals(arg, "--protect-control-groups") {
            if let Some(b) = parse_boolish(value) {
                props.set("ProtectControlGroups", if b { "yes" } else { "no" });
                explicit.protect_control_groups = Some(b);
            }
            i += 1;
            continue;
        }

        // Then --flag value format
        match arg.as_str() {
            "--profile" => {
                i += 1;
                if i < args.len() {
                    apply_profile_by_name(props, &args[i]);
                }
            }
            "-m" | "--memory-limit" => {
                i += 1;
                if i < args.len() {
                    props.set("MemoryMax", &args[i]);
                }
            }
            "-c" | "--cpu-limit" => {
                i += 1;
                if i < args.len() {
                    props.set("CPUQuota", &args[i]);
                }
            }
            "--memory-swap-max" => {
                i += 1;
                if i < args.len() {
                    props.set("MemorySwapMax", &args[i]);
                }
            }
            "--protect-home" => {
                i += 1;
                if i < args.len() {
                    let value = &args[i];
                    let normalized = if value == "none" {
                        "no".to_string()
                    } else {
                        value.to_string()
                    };
                    props.set("ProtectHome", &normalized);
                    explicit.protect_home = Some(normalized);
                }
            }
            "--protect-system" => {
                i += 1;
                if i < args.len() {
                    props.set("ProtectSystem", &args[i]);
                    explicit.protect_system = Some(args[i].clone());
                }
            }
            "--private-tmp" => {
                i += 1;
                if i < args.len() {
                    if let Some(b) = parse_boolish(&args[i]) {
                        props.set("PrivateTmp", if b { "yes" } else { "no" });
                        explicit.private_tmp = Some(b);
                    }
                }
            }
            "--private-devices" => {
                i += 1;
                if i < args.len() {
                    if let Some(b) = parse_boolish(&args[i]) {
                        props.set("PrivateDevices", if b { "yes" } else { "no" });
                        explicit.private_devices = Some(b);
                    }
                }
            }
            "--protect-kernel-tunables" => {
                i += 1;
                if i < args.len() {
                    if let Some(b) = parse_boolish(&args[i]) {
                        props.set("ProtectKernelTunables", if b { "yes" } else { "no" });
                        explicit.protect_kernel_tunables = Some(b);
                    }
                }
            }
            "--protect-control-groups" => {
                i += 1;
                if i < args.len() {
                    if let Some(b) = parse_boolish(&args[i]) {
                        props.set("ProtectControlGroups", if b { "yes" } else { "no" });
                        explicit.protect_control_groups = Some(b);
                    }
                }
            }
            "--current-dir-only" => {
                props.apply_lockdown_base("tmpfs");
            }
            _ => {}
        }

        i += 1;
    }
}

fn apply_defaults(props: &mut SystemdProps, explicit: &ExplicitFlags) {
    // If no lockdown base was applied, apply default protections for flags
    // that were not explicitly set by the user.
    if props.lockdown_base {
        return;
    }
    if explicit.private_tmp.is_none() {
        props.set("PrivateTmp", "yes");
    }
    if explicit.private_devices.is_none() {
        props.set("PrivateDevices", "yes");
    }
    if explicit.protect_kernel_tunables.is_none() {
        props.set("ProtectKernelTunables", "yes");
    }
    if explicit.protect_control_groups.is_none() {
        props.set("ProtectControlGroups", "yes");
    }
}

fn shell_quote(s: &str) -> String {
    if s.contains(' ')
        || s.contains('"')
        || s.contains('\'')
        || s.contains('$')
        || s.contains('&')
        || s.contains('|')
        || s.contains(';')
        || s.contains('<')
        || s.contains('>')
        || s.contains('`')
    {
        format!("'{}'", s.replace('\'', "'\\''"))
    } else {
        s.to_string()
    }
}

// ============ Main ============

fn main() -> Result<()> {
    let cli = Run::parse();

    let mut parts = vec!["systemd-run".to_string()];
    let base_command = "--user --same-dir --wait --pipe";
    parts.extend(base_command.split_whitespace().map(String::from));

    // Include all env vars in the calling environment
    if cli.capture_env {
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

    // Build systemd properties with rightmost-wins semantics
    let mut props = SystemdProps::new();
    let mut explicit = ExplicitFlags::default();

    // Process compound settings and resource limits in command-line order
    process_ordered_args(&mut props, &mut explicit);

    // Apply defaults for flags not explicitly set
    apply_defaults(&mut props, &explicit);

    // Add user-specified path controls (accumulate with profile paths)
    for path in &cli.rw_paths {
        props.push("BindPaths", path.clone());
    }
    for path in &cli.ro_paths {
        props.push("BindReadOnlyPaths", path.clone());
    }
    for path in &cli.inaccessible {
        props.push("InaccessiblePaths", path.clone());
    }

    // Default swap behavior when no profile and no explicit --memory-swap-max
    if props.get("MemoryMax").is_some()
        && props.get("MemorySwapMax").is_none()
        && !props.profile_applied
    {
        props.set("MemorySwapMax", "0");
    }

    // Emit properties
    parts.extend(props.emit());

    parts.extend(cli.command_and_args.clone());

    if cli.dry_run {
        println!(
            "{}",
            parts
                .iter()
                .map(|s| shell_quote(s))
                .collect::<Vec<_>>()
                .join(" ")
        );
        return Ok(());
    }

    let execvp_args: Vec<CString> = parts
        .iter()
        .map(|s| CString::new(s.clone()).unwrap())
        .collect();

    execvp(&execvp_args[0], &execvp_args)?;

    Ok(())
}
