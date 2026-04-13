# zephyrRS

A small `no_std` x86_64 microkernel-ish OS written in Rust, built for a 2023 MSc dissertation ("The implications of an operating system written in Rust", University of Birmingham). It boots via the `bootloader` crate, runs under QEMU, and demonstrates a kernel/user split, per-process page tables, preemptive threading, and synchronous message-passing IPC between userspace and kernel.

This README is aimed at someone cloning the repo cold. The goal: after following the prerequisites, you `cd zephyrRS && make run`, a QEMU window opens, and you get an interactive demo shell.

## What it does

Boot sequence: bootloader → kernel init (GDT, IDT, PICs) → paging + heap → syscall MSR setup → spawn kernel thread → kernel thread loads a user ELF (embedded in the kernel binary via `include_bytes!`) → user main runs in ring 3.

The user program (`zephyrRS/bin/`) is an interactive shell that reads keypresses delivered from the kernel's PS/2 interrupt handler over an IPC socket, and dispatches commands:

| key     | action                                                              |
|---------|---------------------------------------------------------------------|
| `h`     | print help                                                          |
| `b`     | spawn 3 threads that each increment their own counter               |
| `r`     | spawn a thread that recurses + yields, exercising the user stack    |
| `t`     | deeper recursion (intentional stack overflow demo)                  |
| `m`     | spawn a thread that tries to write to another thread's kernel stack |
| `e`     | heap allocate/free demo                                             |
| `n`     | `Option<T>` / null-safety demo                                      |
| `1`–`4` | atomically increment a counter in parallel from N threads           |
| `x`     | exit                                                                |

Output goes to the VGA text buffer (green on black) in the QEMU window. A serial console on stdio shows kernel-side logging including the memory layout, page-fault diagnostics, scheduler events, and per-thread stack usage.

## Repository layout

All source lives under the `zephyrRS/` subdirectory at the repo root. The rest of this repo holds the dissertation PDF and a screenshot of the original working state.

```
zephyrRS/
  kernel/     the kernel binary + library. Submodules:
                boot/    GDT, IDT, PIC, interrupt + syscall entry glue
                dev/     VGA, serial, PS/2 keyboard
                mem/     paging, frame allocator, heap allocators, mem logger
                proc/    processes, threads, scheduler, ELF loader
                syscall  syscall dispatch
                sync     Socket/Message IPC primitives
  api/        userspace runtime crate — panic handler, _start, syscall shims,
              println! macro. User binaries link against this.
  bin/        example user program (the interactive shell above)
  user/bin    compiled user ELF, embedded by the kernel at build time
  x86_64-zephyr.json   custom target (soft-float, no red zone, no SSE)
  makefile    build user ELF, then kernel
Master_s_project.pdf   the original 2023 dissertation
```

## Prerequisites

### 1. Rust toolchain

The repo pins a specific nightly via `zephyrRS/rust-toolchain.toml`. You need `rustup` so that pin is picked up automatically:

```sh
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

Once `rustup` is installed, entering the `zephyrRS/` directory will trigger it to download the pinned nightly plus `rust-src`, `llvm-tools-preview`, `rustfmt`, and `clippy`. No manual `rustup toolchain install` needed.

If you already use a system Rust (Arch `rust`, Homebrew, etc.) and don't want rustup on your PATH, install rustup without modifying your shell (`sh rustup-init.sh -y --no-modify-path`) and invoke it as `~/.cargo/bin/cargo` inside this repo only.

### 2. bootimage

The kernel is turned into a bootable disk image by the `bootimage` tool, invoked automatically as the cargo runner. Install it against the repo's nightly:

```sh
cargo install bootimage
```

### 3. QEMU

You need `qemu-system-x86_64` and a GUI frontend for QEMU to open a window.

- **Arch Linux**: `sudo pacman -S qemu-system-x86 qemu-ui-gtk`
- **Debian/Ubuntu**: `sudo apt install qemu-system-x86`
- **macOS**: `brew install qemu`

Without a QEMU GUI backend (e.g. `qemu-ui-gtk` on Arch), QEMU exits immediately when it can't open a display, and you'll only see the serial log in your terminal with nothing else happening. If that's what you see, install the GUI package.

## Build and run

```sh
cd zephyrRS
make run
```

What that does, in order:
1. `cargo build --release --bin bin` — builds the user program against `api`.
2. Copies the resulting ELF to `zephyrRS/user/bin` so the kernel can `include_bytes!` it.
3. `cargo run --release --bin kernel` — builds the kernel for the custom target, bootimage wraps it into a bootable image, and QEMU runs it.

The first build downloads the pinned nightly and compiles `core` / `compiler_builtins` for the custom target, which takes several minutes. Subsequent builds are fast.

**Important**: always rebuild `user/bin` after editing anything in `bin/` or `api/`. The kernel embeds the file at compile time, so a stale `user/bin` will silently run old userspace code. `make run` handles this for you; if you're invoking cargo directly, run `make user/bin` first.

QEMU is started with `-serial stdio -vga std -gdb tcp::3333`, so:
- Kernel serial logs stream to your terminal.
- A GDB stub is exposed on `localhost:3333` for remote debugging (`target remote :3333` in gdb).

To quit QEMU: close the window, or `Ctrl-C` the terminal running `make run`.

## Tests

Custom test framework built on `#![feature(custom_test_frameworks)]`. Integration tests live in `zephyrRS/kernel/tests/`:

```sh
cd zephyrRS
cargo test                        # all tests
cargo test --test heap_allocation # one integration test
cargo test --bin kernel           # unit tests in the kernel binary
```

Test runs use the `isa-debug-exit` device on port `0xf4` to report status back to `cargo test`; success exit code `0x10` maps to `0`. Test timeout is 300s.

The `should_panic` and `stack_overflow` integration tests need `harness = false` and their `[[test]]` stanzas are currently commented out in `kernel/Cargo.toml` — uncomment before running them.

## Pre-commit checks

Run in this order before every commit, from `zephyrRS/`:

```sh
cargo fmt --all
cargo clippy --all-targets -- -D warnings
cargo test
```

If clippy or tests fail because `user/bin` is stale or missing, run `make user/bin` first and retry — don't skip.

## Known rough edges

- The kernel build currently emits ~10 warnings that need cleaning up before `-D warnings` will pass on the kernel itself.
- User program output goes only to the VGA buffer, not serial, so headless runs can't see what userspace is doing.
- `should_panic` / `stack_overflow` integration test harnesses are commented out.

## Background

The project is based on the 2023 MSc dissertation at the University of Birmingham (included as `Master_s_project.pdf` at the repo root) and was dormant for about two and a half years before being revived in 2026 against a newer nightly. The revival required a handful of concrete changes: target spec rewrite for newer LLVM data layout, `naked_asm!` modernization, `rust-lld` linker flag changes, a latent VGA indexing bug, and moving the user stack region out of a slot that the modern bootloader now reserves. See the git history around 2026-04 for details.
