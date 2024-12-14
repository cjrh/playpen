use clap::Parser;
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

    #[clap()]
    command_and_args: Vec<String>,
}

fn main() -> Result<()> {
    let cli = Run::parse();

    let mut parts = vec!["systemd-run".to_string()];
    let base_command = "--user --same-dir --wait --pipe --quiet";
    parts.extend(
        base_command.split_whitespace().map(String::from)
    );

    // Only add --pty if we are attached to a terminal
    if atty::is(Stream::Stdout) && atty::is(Stream::Stdin) {
        parts.push("--pty".to_string());
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
