![playpen](playpen_transparent2.png)

Download button: [![Download](https://api.bintray.com/packages/anderspitman/generic/playpen/images/download.svg) ](https://bintray.com/anderspitman/generic/playpen/_latestVersion)

# playpen
Program launcher with memory and cpu limits

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
      --capture-env <CAPTURE_ENV>  [default: false] [possible values: true, false]
      --capture-path <CAPTURE_PATH>                [default: true] [possible values: true, false]
  -h, --help                                       Print help
  -V, --version                                    Print version
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
