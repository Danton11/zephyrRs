# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Overview

ZephyrRS is a `no_std` x86_64 hobby kernel written in Rust, booted via the `bootloader` crate and run under QEMU through `bootimage`. It has a custom target (`x86_64-zephyr.json`, soft-float, no red zone) and builds with `build-std` against nightly Rust.

## Workspace Layout

Cargo workspace with three members:

- `kernel/` — the kernel binary. Entry `kernel/src/main.rs`, library surface in `kernel/src/lib.rs`. Submodules:
  - `boot/` — GDT, IDT, PIC init, interrupt handlers (`interrupts.rs` also exposes the keyboard socket).
  - `dev/` — VGA text buffer, serial (UART 16550), PS/2 keyboard.
  - `mem/` — `memory.rs` (paging, frame allocator, heap mapping using bootloader's physical memory map) and `allocator/` (bump, linked list, fixed-size block heap allocators). `memory/memory_logger.rs` backs the global `MEMORYLOGGER`.
  - `proc/process.rs` — threads and processes. Loads user ELFs via the `object` crate, sets up per-process page tables and user heaps, exposes `spawn_kernel_thread` / `spawn_user_thread`. Cooperative/preempted scheduling via `WAIT_QUEUE` + `RUNNING_THREAD` under spin `RwLock`s. Constants: `KERNEL_STACK_SIZE`, `USER_STACK_SIZE`, `USER_CODE_START`, user heap base/size.
  - `syscall.rs` — syscall dispatch (yield, exit, spawn, send, receive, ...). Wired up from `kernel_main` after `kernel::init()` and `memory::init(boot_info)`.
  - `sync.rs` — `Socket` / `Message` / `Data` IPC primitives used for all kernel↔user and inter-thread communication.
- `api/` — userspace runtime crate (`no_std`, panic handler, `_start` that reads heap base/size from `rax`/`rcx`, sets up `linked_list_allocator`, then calls user `main`). Provides `syscall`, `mem`, and `print` modules plus a `println!` macro; user binaries link against this.
- `bin/` — example user program built with the custom target against `api`. The kernel embeds its compiled output at `user/bin` via `include_bytes!` in `kernel/src/main.rs::kernel_thread_main`.

Four global sockets live in `kernel::lib.rs`: `ID_SOCKET` (kernel→user thread-ID channel), `PROC_FIN_SOCKET`, `FIN_SOCKET`, and per-device sockets returned by `keyboard_socket()` / `vga_buffer::start_listener()`. When spawning a user thread, the kernel passes a vector of socket handles as file-descriptor-like resources (see `kernel_thread_main`); userspace references them by numeric index in `syscall::send` / `syscall::receive`.

## Build & Run

The project requires **nightly Rust** plus `bootimage`, `llvm-tools-preview`, and `rust-src` components. `.cargo/config.toml` sets the default target to `x86_64-zephyr.json` and uses `bootimage runner` as the test runner.

Build + run (QEMU) — this is the canonical workflow:

```
make run
```

`make run` builds the user binary via `make user/bin` (which runs `cargo build --release --bin bin` and copies the artifact to `user/bin` so the kernel can embed it), then runs `cargo run --release --bin kernel`. The kernel binary **will not build** unless `user/bin` exists, because `kernel/src/main.rs` uses `include_bytes!("../../user/bin")`. After editing anything in `bin/` or `api/`, rerun `make user/bin` before rebuilding the kernel.

QEMU run args (from `kernel/Cargo.toml` `package.metadata.bootimage`): `-serial stdio -vga virtio -gdb tcp::3333` — a gdb stub is exposed on port 3333.

## Tests

Custom test framework (`#![feature(custom_test_frameworks)]`, `test_runner = kernel::test_runner`). Integration tests live in `tests/` (`basic_boot.rs`, `heap_allocation.rs`, `should_panic.rs`, `stack_overflow.rs`). QEMU exits via `isa-debug-exit` at port `0xf4`; success code `0x10` is mapped to exit 0 via `test-success-exit-code = 33`. Test timeout is 300s.

```
cargo test                          # all tests
cargo test --test heap_allocation   # single integration test
cargo test --bin kernel             # unit tests in the kernel binary
```

Note: `should_panic` and `stack_overflow` need `harness = false` to run correctly; those `[[test]]` stanzas are currently commented out in `kernel/Cargo.toml` — uncomment before running them.

## Analysis Scripts

`filter_logs.sh` and the `mem_*.py` / `stack_reuse.py` / `visualise_regions.py` / `visual.py` scripts at the repo root consume serial output from `MEMORYLOGGER` to produce memory-fragmentation / region plots. They are ad-hoc tooling, not part of the build.

## Working Principles

- **Correctness over speed.** Don't ship working-but-unidiomatic code to finish faster. Refactoring to accommodate the right pattern is always acceptable.
- **Shift-left testing.** When changing kernel/user code, actively ask "what would have caught this earlier?" Prefer extracting pure logic out of interrupt/IO paths into unit-testable helpers rather than debugging exclusively via QEMU runs. Add test cases alongside changes; call out areas that genuinely can't be tested instead of silently skipping.
- **Error handling.** Propagate with `?`; surface errors to callers. `.expect("reason")` only for proven invariants; no `.unwrap()` in production paths. When deliberately discarding an error, use `if let Err(e) = ...` with a comment — never silent `let _ =`.
- **Reintroduction mode.** This project has been dormant; when touching unfamiliar subsystems, re-read the surrounding module before editing and briefly explain architectural context in responses rather than assuming recall.
- **Standalone-ability is an active goal.** Treat "can a stranger clone and run this" as a design constraint. Flag implicit setup knowledge (nightly channel, `make user/bin` ordering, missing `rust-toolchain.toml`, etc.) when you encounter it.

## Pre-commit checks

Run in order before every commit:

```
cargo fmt --all
cargo clippy --all-targets -- -D warnings
cargo test
```

If clippy/test fail because `user/bin` is stale or missing (the kernel `include_bytes!`s it), run `make user/bin` first and retry — don't skip the checks. Fix clippy warnings rather than suppressing; any `#[allow(...)]` needs a comment explaining why.

## Style

- Modules under ~400 lines; split when exceeded.
- `impl Trait` in argument position for single-use generics; explicit generics when the type appears in multiple positions or a return.
- No dep for what `core`/`alloc` provides trivially. Pin major versions (`"1"` not `"*"`).

## Gotchas

- Always rebuild `user/bin` before the kernel; a stale or missing `user/bin` produces either old behavior or a build error from `include_bytes!`.
- All kernel synchronization uses `spin::RwLock` / `spin::Mutex` — no blocking. Touching `RUNNING_THREAD` / `WAIT_QUEUE` from interrupt context must go through `x86_64::instructions::interrupts::without_interrupts`.
- User ELFs are loaded by segment via the `object` crate into a fresh page table; `USER_CODE_START = 0x5000000` and the user heap lives at `0x280_0060_0000` (4 MiB). Keep user linker output consistent with these.
- The custom target disables SSE/MMX and uses soft-float; do not introduce code that assumes hardware float.
