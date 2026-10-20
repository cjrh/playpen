use std::fs;
use std::os::unix::fs::{symlink, PermissionsExt};
use std::process::Command;

mod common;

#[test]
#[ignore = "requires systemd mount namespaces and python3; run locally with --include-ignored"]
fn agent_boundary_and_npm_symlinks_work_in_a_real_service() {
    // Use a synthetic home under /home so ProtectHome masks the sentinel.
    // No real credentials or agent configuration are read or modified.
    let home = tempfile::Builder::new()
        .prefix(".playpen test %-")
        .tempdir_in(std::env::var_os("HOME").unwrap())
        .unwrap();
    for path in [
        "project",
        ".agents/skills",
        ".codex",
        ".claude",
        ".config/gh",
        "custom-tools",
        "prefix/bin",
        "prefix/lib/node_modules/agent",
    ] {
        fs::create_dir_all(home.path().join(path)).unwrap();
    }
    fs::write(home.path().join("hidden-sentinel"), "private").unwrap();
    fs::write(
        home.path().join(".agents/instructions.md"),
        "shared instructions",
    )
    .unwrap();
    fs::write(
        home.path().join(".config/gh/config.yml"),
        "synthetic gh config",
    )
    .unwrap();
    fs::write(home.path().join(".config/unrelated-secret"), "hidden").unwrap();
    fs::write(home.path().join("custom-tools/private-tool"), "hidden").unwrap();
    let gh = home.path().join("custom-tools/gh");
    fs::write(
        &gh,
        r#"#!/usr/bin/python3
import errno, os, pathlib, tempfile
config = pathlib.Path(os.environ['GH_CONFIG_DIR'])
assert (config / 'config.yml').read_text() == 'synthetic gh config'
assert os.environ['GH_TOKEN'] == 'synthetic gh token $literal with spaces'
assert not (config.parent / 'unrelated-secret').exists()
assert not pathlib.Path(__file__).with_name('private-tool').exists()
try:
    with tempfile.NamedTemporaryFile(dir=config): pass
except OSError as e:
    assert e.errno in (errno.EROFS, errno.EACCES, errno.EPERM), e
else:
    raise AssertionError('gh config is writable')
"#,
    )
    .unwrap();
    fs::set_permissions(&gh, fs::Permissions::from_mode(0o755)).unwrap();
    let entry = home.path().join("prefix/lib/node_modules/agent/cli.py");
    fs::write(entry.with_file_name("sibling.txt"), "imports work").unwrap();
    fs::write(&entry, r##"#!/usr/bin/python3
import errno, os, pathlib, socket, subprocess, sys, tempfile
home = pathlib.Path.home()
assert not (home / 'hidden-sentinel').exists()
assert not pathlib.Path('/run/user/%d/bus' % os.getuid()).exists()
assert 'PLAYPEN_TEST_SECRET' not in os.environ
assert sys.argv[1] == '$UNCHANGED'
assert pathlib.Path(__file__).resolve().with_name('sibling.txt').read_text() == 'imports work'
assert (home / '.agents/instructions.md').read_text() == 'shared instructions'
subprocess.run(['gh'], check=True)
for path in [pathlib.Path.cwd(), home / '.codex', home / '.claude', pathlib.Path('/tmp')]:
    with tempfile.NamedTemporaryFile(dir=path) as f:
        f.write(b'probe')
for path in [home / '.agents', home / 'prefix/bin', pathlib.Path('/usr')]:
    try:
        with tempfile.NamedTemporaryFile(dir=path): pass
    except OSError as e:
        assert e.errno in (errno.EROFS, errno.EACCES, errno.EPERM), e
    else:
        raise AssertionError('unexpectedly writable: ' + str(path))
# Claude can create sibling lock files and atomically replace its new config.
config = pathlib.Path(os.environ['CLAUDE_CONFIG_DIR']) / '.claude.json'
config.with_suffix('.tmp').write_text('{}')
config.with_suffix('.tmp').replace(config)
with socket.socket(socket.AF_UNIX) as client:
    try:
        client.connect('/run/docker.sock')
    except OSError as e:
        assert e.errno in (errno.ENOENT, errno.EACCES, errno.EPERM, errno.ENOTSOCK, errno.ECONNREFUSED), e
    else:
        raise AssertionError('Docker socket is accessible')
group = next(line.split(':', 2)[2] for line in pathlib.Path('/proc/self/cgroup').read_text().splitlines() if line.startswith('0::'))
cgroup = pathlib.Path('/sys/fs/cgroup') / group.lstrip('/')
assert (cgroup / 'memory.max').read_text().strip() == str(4 * 1024**3)
assert (cgroup / 'memory.swap.max').read_text().strip() == '0'
assert (cgroup / 'cpu.max').read_text().split() == ['200000', '100000']
print('PASS: filesystem, environment, sockets, resource limits, npm links, Claude atomic writes')
"##).unwrap();
    fs::set_permissions(&entry, fs::Permissions::from_mode(0o755)).unwrap();
    symlink(&entry, home.path().join("prefix/bin/pi")).unwrap();
    symlink(
        home.path().join("prefix"),
        home.path().join("runtime-alias"),
    )
    .unwrap();
    let output = Command::new(common::get_playpen_path())
        .current_dir(home.path().join("project"))
        .env("HOME", home.path())
        .env_remove("GH_CONFIG_DIR")
        .env_remove("XDG_CONFIG_HOME")
        .env(
            "PATH",
            format!(
                "{}/runtime-alias/bin:{}/custom-tools:/usr/bin:/bin",
                home.path().display(),
                home.path().display()
            ),
        )
        .env("PLAYPEN_TEST_SECRET", "never-forward")
        .env("GH_TOKEN", "synthetic gh token $literal with spaces")
        .args(["--profile", "coding-agent", "--", "pi", "$UNCHANGED"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("PASS:"));
}
