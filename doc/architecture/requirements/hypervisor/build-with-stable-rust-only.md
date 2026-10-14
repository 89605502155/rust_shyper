---
kind: non-functional
origin: intent
satisfied-by: [Hypervisor]
deferred:
---

# Build with stable Rust only

The whole project builds with one pinned stable Rust toolchain and no other tools.

## System Context

rust-toolchain pins Rust 1.99 with its targets and components.

## Satisfy

Built-in targets, rust-lld and llvm-objcopy replace custom targets, build-std and GNU binutils; no nightly feature is used.

- test — `tools/check.sh` builds and lints every board with the pinned toolchain
