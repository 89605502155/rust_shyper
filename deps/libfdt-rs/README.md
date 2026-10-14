# libfdt-rs

Flattened device tree editing for Rust-Shyper, written in pure Rust.

It started as <https://github.com/Zera-Algorithm/libfdt-rs> (commit 44570fb),
which wrapped the C libfdt from dtc through bindgen. The part of libfdt used by
the hypervisor (`fdt_ro.c`, `fdt_rw.c`, `fdt_wip.c`, `fdt_overlay.c`) and the
C helpers from `wrapper.c` were ported to Rust, so no C compiler, bindgen or
libclang is needed any more and it builds on stable Rust.

* `raw` - the libfdt port, working on `&[u8]` / `&mut [u8]` blobs and returning
  libfdt error codes;
* `overlay` - device tree overlay application;
* `wrapper` - the device tree patching helpers with the old C interface;
* `binding` - `Fdt` / `FdtBuf` types.

Run the unit tests with `cargo test --target x86_64-unknown-linux-gnu`.
