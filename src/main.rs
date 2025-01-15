use clap::Parser;
use clap::builder::BoolishValueParser;
use clap::builder::TypedValueParser as _;
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
    capture_environment: bool,

    #[arg(long, action = ArgAction::Set, value_parser = BoolishValueParser::new(), default_value = "true")]
    capture_path: bool,

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
    if cli.capture_environment {
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

    parts.extend(cli.command_and_args.clone());

    let execvp_args: Vec<CString> = parts
        .iter()
        .map(|s| CString::new(s.clone()).unwrap())
        .collect();

    execvp(&execvp_args[0], &execvp_args)?;

    Ok(())
}
