use clap::Parser;
use clap::builder::BoolishValueParser;
use clap::ArgAction;
use nix::unistd::execvp;
use std::ffi::CString;
use anyhow::Result;
use atty::Stream;

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

    #[arg(long, default_value = "none", help = "Protect home directories: none/yes/read-only/tmpfs")]
    protect_home: String,

    #[arg(long, default_value = "none", help = "Protect system directories: none/yes/full/strict")]
    protect_system: String,

    // Preset configurations
    #[arg(long, default_value = "false", help = "Restrictive preset: only current directory accessible")]
    current_dir_only: bool,

    #[clap()]
    command_and_args: Vec<String>,
}

fn main() -> Result<()> {
    let cli = Run::parse();

    let mut parts = vec!["systemd-run".to_string()];
    let base_command = "--user --same-dir --wait --pipe";
    parts.extend(
        base_command.split_whitespace().map(String::from)
    );

    // Include all env vars in the calling environment
    if cli.capture_env {
        for (key, value) in std::env::vars() {

            // Skip the environment variables that systemd-run sets
            if key == "DBUS_SESSION_BUS_ADDRESS" {
                continue;
            }

            // Skip env vars that are actually exported bash functions
            // These are not supported by systemd-run.
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

    if let Some(memory_limit) = cli.memory_limit {
        parts.push(format!("-pMemoryMax={}", memory_limit));
        parts.push("-pMemorySwapMax=0".to_string());
    }

    if let Some(cpu_limit) = cli.cpu_limit {
        parts.push(format!("-pCPUQuota={}", cpu_limit));
        parts.push("-pCPUQuotaPeriodSec=100ms".to_string());
    }

    // Handle preset configurations first
    if cli.current_dir_only {
        // Apply basic protections without filesystem restrictions that conflict with --same-dir
        parts.push("-pPrivateTmp=yes".to_string());
        parts.push("-pPrivateDevices=yes".to_string());
        parts.push("-pProtectKernelTunables=yes".to_string());
        parts.push("-pProtectControlGroups=yes".to_string());

        // Block access to sensitive directories while allowing system functionality
        // Use ProtectHome to block access to home directories
        parts.push("-pProtectHome=yes".to_string());

        // Allow current directory via bind mount
        let pwd = std::env::current_dir()?;
        parts.push(format!("-pBindPaths={}", pwd.display()));
    } else {
        // Apply individual protection flags if not using preset
        if cli.private_tmp {
            parts.push("-pPrivateTmp=yes".to_string());
        }
        if cli.private_devices {
            parts.push("-pPrivateDevices=yes".to_string());
        }
        if cli.protect_kernel_tunables {
            parts.push("-pProtectKernelTunables=yes".to_string());
        }
        if cli.protect_control_groups {
            parts.push("-pProtectControlGroups=yes".to_string());
        }
        if cli.protect_home != "none" {
            parts.push(format!("-pProtectHome={}", cli.protect_home));
        }
        if cli.protect_system != "none" {
            parts.push(format!("-pProtectSystem={}", cli.protect_system));
        }
    }

    // Apply fine-grained path controls (these can override or extend preset configurations)
    for path in &cli.rw_paths {
        parts.push(format!("-pBindPaths={}", path));
    }

    for path in &cli.ro_paths {
        parts.push(format!("-pBindReadOnlyPaths={}", path));
    }

    for path in &cli.inaccessible {
        parts.push(format!("-pInaccessiblePaths={}", path));
    }

    parts.extend(cli.command_and_args.clone());

    let execvp_args: Vec<CString> = parts
        .iter()
        .map(|s| CString::new(s.clone()).unwrap())
        .collect();

    execvp(&execvp_args[0], &execvp_args)?;

    Ok(())
}
