// systemd-run --user -p MemoryMax=1G your_command_here

use clap::{Arg, Command, ArgAction};
use nix::unistd::{execvp, getpid};
use std::ffi::CString;
use std::fs::{self, File};
use std::io::Write;
use std::path::PathBuf;

// const CGROUP_BASE: &str = "/sys/fs/cgroup/memory"; // cgroup v1 memory controller base path
// const CGROUP_BASE: &str = "/sys/fs/cgroup/user.slice/user-1000.slice/session-c2.scope";
const CGROUP_BASE: &str = "/sys/fs/cgroup/user.slice/user-1000.slice/user@1000.service";
const GROUP_PREFIX: &str = "mylauncher_";

// TODO:
//  1. Check that the memory controller is enabled in the parent cgroup
//     Otherwise, you will have to enable it with:
//     echo "+memory" > /sys/fs/cgroup/user.slice/user-1000.slice/session-c2.scope/cgroup.subtree_control
//  2. Automate looking for and finding the user delegated cgroup path, if possible.
//     If it's not possible, we'll have to set one up. This can be done with sudo,
//     but we'll likely also have to record the name of the path somewhere in ~/.config
//     or something like that to find it again.


fn parse_memory_limit(mem_str: &str) -> Result<u64, String> {
    // A simplistic parser. Accepts values like "1G", "512M", "2048".
    // In a production scenario, handle all units and errors gracefully.
    let mem_str = mem_str.trim().to_uppercase();
    if let Some(idx) = mem_str.find(|c: char| !c.is_ascii_digit()) {
        let (num_str, unit) = mem_str.split_at(idx);
        let num: u64 = num_str.parse().map_err(|e| format!("{e:?}"))?;
        let bytes = match unit {
            "G" => num * 1024 * 1024 * 1024,
            "M" => num * 1024 * 1024,
            "K" => num * 1024,
            _ => return Err("Unknown unit".to_string()),
        };
        Ok(bytes)
    } else {
        // Pure number means bytes directly
        mem_str.parse().map_err(|e| format!("{e:?}"))
    }
}

fn slugify_command(prog: &str, args: &[String]) -> String {
    let mut combined = vec![prog.to_string()];
    combined.extend(args.iter().cloned());

    // Simple slug: remove non-alphanumeric, join with underscore
    let joined = combined.join("_");
    let slug: String = joined.chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect();
    slug
}

/// cgroups v1 interface
fn create_cgroup_v1(name: &str, mem_limit_bytes: u64) -> std::io::Result<()> {
    let group_path = PathBuf::from(CGROUP_BASE).join(name);
    fs::create_dir(&group_path)?;
    // Write memory limit
    {
        let mut f = File::create(group_path.join("memory.limit_in_bytes"))?;
        writeln!(f, "{}", mem_limit_bytes)?;
    }
    // Add current PID to tasks
    {
        let mut f = File::create(group_path.join("tasks"))?;
        writeln!(f, "{}", getpid().as_raw())?;
    }

    Ok(())
}

/// cgroups v2 interface
fn create_cgroup(name: &str, mem_limit_bytes: u64) -> std::io::Result<()> {
    use std::io::{Error, ErrorKind};

    fn err(msg: String) -> Error {
        Error::new(ErrorKind::Other, msg)
    }

    let group_path = PathBuf::from(CGROUP_BASE).join(name);

    if !group_path.exists() {
        // If the cgroup does not exist, create it
        fs::create_dir(&group_path)
            .map_err(|e|
                err(format!("Failed creating dir: {group_path:?} {e:?}"))
            )?;
    }

    // Set memory limit using cgroup v2 interface file
    let mem_max_path = group_path.join("memory.max");
    if !mem_max_path.exists() {
        let mut f = File::create(group_path.join("memory.max"))
            .map_err(|e|
                err(format!("Failed creating memory.max: {group_path:?} {e:?}"))
            )?;
        writeln!(f, "{}", mem_limit_bytes)?;
    }

    // Add the current PID to the cgroup
    // cgroup v2 uses `cgroup.procs` instead of `tasks`
    let procs_path = group_path.join("cgroup.procs");
    let mut f = File::create(procs_path)
        .map_err(|e|
            err(format!("Failed creating cgroup.procs: {group_path:?} {e:?}"))
        )?;
    writeln!(f, "{}", getpid().as_raw())
        .map_err(|e|
            err(format!("Failed writing to cgroup.procs: {group_path:?} {e:?}"))
        )?;

    Ok(())
}

fn remove_created_cgroups() -> std::io::Result<()> {
    // Lists cgroups under /sys/fs/cgroup/memory, removes those starting with `mylauncher_`
    for entry in fs::read_dir(CGROUP_BASE)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with(GROUP_PREFIX) {
                let group_path = entry.path();
                // Attempt to remove the cgroup directory
                // First, ensure no tasks inside (if the launched program ended, tasks should be empty)
                // For simplicity, just remove directly assuming no tasks:
                fs::remove_dir(&group_path).ok();
            }
        }
    }
    Ok(())
}

fn main() -> std::io::Result<()> {
    let matches = Command::new("cgroup_launcher")
        .version("0.1.0")
        .author("You <you@example.com>")
        .about("Launches a program under a memory-constrained cgroup")
        .subcommand_required(false)
        .arg_required_else_help(true)
        .subcommand(
            Command::new("run")
                .about("Run a command with memory limit")
                .arg(Arg::new("memory-limit")
                     .short('m')
                     .long("memory-limit")
                     .value_name("MEM")
                     .help("Memory limit (e.g., 1G, 512M, etc.)")
                     .required(true))
                .arg(
                    Arg::new("cpu-limit")
                        .short('c')
                        .long("cpu-limit")
                        .value_name("CPU")
                        .help("Optional CPU limit (e.g., '50%' or '2' cores)")
                        .required(false)
                )
                .arg(Arg::new("command")
                     .help("Command to run")
                     .required(true))
                .arg(Arg::new("args")
                     .help("Arguments for the command")
                     // .multiple_occurrences(true)
                     // .num_args
                     .required(false)
                     .action(ArgAction::Append))
        )
        .subcommand(
            Command::new("clear")
                .about("Clear previously created cgroups")
        )
        .get_matches();

    if let Some(run_matches) = matches.subcommand_matches("run") {
        let mem_limit_str = run_matches.get_one::<String>("memory-limit").expect("required by clap");
        let mem_limit = parse_memory_limit(mem_limit_str)
            .expect("Invalid memory limit format");
        let cpu_limit: Option<String> = run_matches.get_one::<String>("cpu-limit").cloned();

        let prog = run_matches.get_one::<String>("command").expect("required by clap");
        let args: Vec<String> = run_matches
            .get_many::<String>("args")
            .map(|vals|
                vals.map(String::clone).collect::<Vec<_>>()
            ).unwrap_or_default();

        println!("Running {:?} with args {:?}, memory limit {}", prog, args, mem_limit);
        let slug = slugify_limits(
            mem_limit_str,
            cpu_limit.as_deref(),
        );
        let group_name = format!("{}{}", GROUP_PREFIX, slug);
        println!("Creating cgroup {:?}", group_name);
        create_cgroup(&group_name, mem_limit)?;
        println!("Created cgroup {:?}", group_name);

        // Now that we are in the cgroup (since we wrote our own PID when
        // we created the cgroup), we can exec the program.
        let mut cmd_args = vec![CString::new(prog.as_str()).unwrap()];
        for a in &args {
            cmd_args.push(CString::new(a.as_str()).unwrap());
        }

        // The really neat trick here is that we transform into the program
        // we were asked to run, so the cgroup limits now apply to it.
        execvp(&cmd_args[0], &cmd_args)
            .expect("Failed to exec the target program");
    } else if matches.subcommand_matches("clear").is_some() {
        remove_created_cgroups()?;
    }

    Ok(())
}

fn slugify_limits(mem_limit: &str, cpu_limit: Option<&str>) -> String {
    // Build a base string that includes the memory limit, and optionally the CPU limit.
    let base_str = if let Some(cpu) = cpu_limit {
        format!("mem_{}_cpu_{}", mem_limit, cpu)
    } else {
        format!("mem_{}", mem_limit)
    };

    // Convert to a "slug" by replacing non-alphanumeric characters (other than '_') with '_'.
    base_str.chars()
        .map(|c| if c.is_alphanumeric() || c == '_' { c } else { '_' })
        .collect()
}

