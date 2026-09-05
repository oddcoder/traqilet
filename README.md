# traqilet

**Build tracking applets for debugging and observing live Linux kernels.**

|  |  |  |  |
| --- | --- | --- | --- |
| [![fmt](https://github.com/oddcoder/traqilet/actions/workflows/fmt.yml/badge.svg?branch=main)](https://github.com/oddcoder/traqilet/actions/workflows/fmt.yml) | [![build](https://github.com/oddcoder/traqilet/actions/workflows/build.yml/badge.svg?branch=main)](https://github.com/oddcoder/traqilet/actions/workflows/build.yml) | [![test](https://github.com/oddcoder/traqilet/actions/workflows/test.yml/badge.svg?branch=main)](https://github.com/oddcoder/traqilet/actions/workflows/test.yml) | [![clippy](https://github.com/oddcoder/traqilet/actions/workflows/clippy.yml/badge.svg?branch=main)](https://github.com/oddcoder/traqilet/actions/workflows/clippy.yml) |
| [![deny](https://github.com/oddcoder/traqilet/actions/workflows/deny.yml/badge.svg?branch=main)](https://github.com/oddcoder/traqilet/actions/workflows/deny.yml) | [![notice](https://github.com/oddcoder/traqilet/actions/workflows/notice.yml/badge.svg?branch=main)](https://github.com/oddcoder/traqilet/actions/workflows/notice.yml) | [![advisories](https://github.com/oddcoder/traqilet/actions/workflows/advisories.yml/badge.svg)](https://github.com/oddcoder/traqilet/actions/workflows/advisories.yml) | [![coverage](https://github.com/oddcoder/traqilet/actions/workflows/coverage.yml/badge.svg?branch=main)](https://github.com/oddcoder/traqilet/actions/workflows/coverage.yml) |
| [![prune](https://github.com/oddcoder/traqilet/actions/workflows/prune.yml/badge.svg)](https://github.com/oddcoder/traqilet/actions/workflows/prune.yml) |  |  |  |

![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)

Traqilet is a small language designed to compile directly to eBPF with
Cranelift.

## The idea

[BCC](https://github.com/iovisor/bcc) lets you write eBPF programs in an
unnamed restricted subset of C, while
[bpftrace](https://github.com/bpftrace/bpftrace) is hard to beat for Perl-style
one-liners. Traqilet targets the space between them: tasks that outgrow a
one-liner but still need fast iteration, portability, and a small binary
footprint.

It aims to fill that gap by treating a tracking applet as a small, self-contained
program with a standard library. Probe attachments are attributes on ordinary
functions, and maps declare their key and value types and capacity where they
are created. Kernel-backed types are not compiler magic: they are described in
the language itself. For example, [`stdlib/hashmap.tql`](stdlib/hashmap.tql)
links the same `HashMap` operations to host syscalls and in-kernel helpers.

The other part of the experiment is the compiler. Both existing approaches
bring a production compiler to the target system: BCC compiles C at runtime
using Clang and LLVM, while bpftrace uses LLVM to compile its own language.
Traqilet instead aims to pair a dedicated Cranelift BPF backend with type
information from the running kernel's BTF. The goal is a self-contained,
easy-to-use tracing tool without a C toolchain or matching kernel headers on the
target machine.

> [!CAUTION]
> Traqilet is a compiler project under construction, not a usable tracer yet.
> It can parse programs and read kernel BTF; it cannot compile or attach eBPF
> programs yet.

## A tracking applet

```tql
#!/usr/bin/env traqilet

start = hashmap(u64, u64, 10240);
buckets = hist(128);

#[kprobe(linux.vfs_read)]
fn enter() {
    start[linux.tid] = linux.monotonic_ns;
}

#[kretprobe(linux.vfs_read)]
fn ret() {
    if !(linux.tid in start) { return; }

    t0 = start[linux.tid];
    buckets.add(linux.monotonic_ns - t0);
    start.delete(linux.tid);
}

#[exit]
fn report() {
    info(buckets);
}
```

This is a complete Traqilet script for measuring `vfs_read` latency. The entry
probe remembers when each thread began a read, the return probe records the
elapsed time, and the exit handler prints the histogram. The parser accepts it
today; compilation and execution are still being built (although I have AI slop
that shows a path to green).

## What Traqilet is for

Traqilet is being built as one language for short-lived diagnostics,
long-running observability, and kernel hacking. A tracking applet will be able
to:

- instrument kernel functions with kprobes, kretprobes, fentry, and fexit
- trace application and library functions with uprobes and uretprobes
- subscribe to kernel tracepoints, raw tracepoints, and userspace USDT probes
- sample software and hardware performance counters using perf events
- count events, measure latency, collect stack traces, and build histograms
- stream structured events to the host or retain state in BPF maps
- prototype schedulers with `sched_ext`
- attach to networking, cgroup, and security hooks such as XDP, TC, and LSM

The aim is to let these attachment points share the same types, maps, functions,
and output model instead of making each kind of BPF program a separate tool.

## Road to a first trace

- [x] Lexer, parser, AST, and source diagnostics
- [x] BTF reader for files and the running kernel
- [x] CLI plumbing
- [ ] Name resolution and type checking
- [ ] Lowering to Cranelift IR
- [ ] Cranelift BPF backend and object emission
- [ ] Map creation, program loading, and probe attachment
- [ ] Host-side handlers and output

## Build

Traqilet requires patching Cranelift. Bootstrap the workspace through the
repository's `xtask`:

```console
$ cargo xtask build
```

The first build fetches a pinned Wasmtime revision into `.patched/`, applies the
patch series in [`patches/`](patches), and then builds the workspace. After that,
ordinary Cargo commands work:

```console
$ cargo test --workspace --locked
$ cargo clippy --workspace --all-targets --locked -- -D warnings
```

## License

Traqilet is licensed under the Apache License 2.0. See [LICENSE](LICENSE) and
[THIRD-PARTY.txt](THIRD-PARTY.txt).
