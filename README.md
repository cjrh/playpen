![playpen](playpen_transparent2.png)
![playpen](playpen.jpg)

# playpen
Program launcher with memory and cpu limits

## Overview

I wanted a way to run a program with memory and CPU limits. It turns out
that `systemd-run` does exactly this, _and it doesn't require root_, but
the necessary parameters are many and confusing. So playpen wraps all that
up in a simple CLI which is set up for the typical use cases I have.

## CLI Docs

```
Usage: playpen [OPTIONS] [COMMAND_AND_ARGS]...

Arguments:
  [COMMAND_AND_ARGS]...  

Options:
  -m, --memory-limit <MEMORY_LIMIT>  
  -c, --cpu-limit <CPU_LIMIT>        
  -h, --help                         Print help
  -V, --version                      Print version
```

## Example

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

A CPU limit can also be set:

```
$ playpen -m 50M -c 100% python3
```

For CPU, "100%" means 1 core, "200%" means 2 cores, and so on.

## Dependencies

This only works on Linux and requires the `systemd` service manager.
In particular, it uses the `systemd-run` command to launch the
processes in a cgroup with the given limits. As such, `playpen`
is a shallow wrapper around `systemd-run`.
