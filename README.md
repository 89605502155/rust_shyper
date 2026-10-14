# trosna-shyper

[Русская версия](README.ru.md)

**trosna-shyper** is a type-1 (bare-metal) embedded hypervisor written in
Rust, for AArch64 and RISC-V. It is based on
[Rust-Shyper](https://github.com/openeuler-mirror/rust_shyper) (Beihang
University, Huawei).

Compared with Rust-Shyper, trosna-shyper:

- builds with **stable Rust 1.99**; upstream needed nightly-2023-11-09;
- has **no C code and no FFI**. It needs **only the Rust toolchain**: no
  cross gcc, binutils or libclang;
- works with plain `cargo build` / `cargo run`;
- fixes a number of bugs;
- comes with a ready Fedora 42 environment for a management VM and a guest VM.

## Quick start

On a Linux host with [rustup](https://rustup.rs), `qemu-system-aarch64`
and docker:

```sh
./tools/mkrootfs.sh      # MVM disk vm0.img: Fedora 42 (AArch64) + a guest VM
cargo run --release      # build and boot the hypervisor in QEMU
```

The hypervisor starts the management VM (MVM). At the `[mvm ~]#` prompt:

```sh
/root/start-gvm.sh             # configure and boot guest VM 1
/root/shyper vm console        # its console (Ctrl-] goes back)
/root/shyper vm list
```

Leave QEMU with `Ctrl-A`, `X`.

## Documentation

- [Operations guide (English)](doc/en/guide.md): building, running, guest
  VMs, configuration, CLI, checks, troubleshooting
- [Руководство по эксплуатации (русский)](doc/ru/guide.md)
- [Architecture model](doc/architecture/README.md), recovered and checked with `archi`
- [Rust-Shyper documentation](doc/upstream/README.md), kept from upstream

## Platforms

| Platform | Status |
|----------|--------|
| QEMU `virt`, AArch64 (GICv2/GICv3) | runs: MVM and guest VMs |
| NVIDIA Jetson TX2, Raspberry Pi 4, Firefly ROC-RK3588S-PC | build and pass the checks (not run on hardware in this project) |
| QEMU `virt`, RISC-V 64 (PLIC/AIA) | builds and passes the checks |

## Checks

```sh
./tools/check.sh    # rustfmt, clippy for every board, CLI, unit tests
```

## License

trosna-shyper is licensed under the [Mulan PSL v2](LICENSE), like Rust-Shyper.
The device tree code in `deps/libfdt-rs` that was ported from libfdt is used
under the BSD 2-Clause license, see [its LICENSE](deps/libfdt-rs/LICENSE).
See [NOTICE](NOTICE) for the origin of the code.
