---
kind: non-functional
origin: intent
satisfied-by: [Hypervisor.Fdt, Mvm.Cli]
deferred:
---

# No C code or FFI

Neither the hypervisor nor the CLI contains C code or calls C libraries.

## System Context

Device tree editing was the C libfdt and the CLI used libc and libfdisk.

## Satisfy

libfdt is ported to Rust, the CLI uses Linux system calls through rustix.

- type-level — no `extern "C"` function declarations other than linker symbols
