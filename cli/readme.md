# shyper: the trosna-shyper management tool

`shyper` runs in the management VM (MVM). It configures, boots and stops
guest VMs, serves their mediated disks and attaches to their consoles,
through `/dev/shyper` of the `shyper.ko` kernel module.

It is written in Rust only and uses Linux system calls through `rustix`;
no C library (`libfdisk`, `libc` calls) is needed. Cross compile it on the
host with the Rust toolchain alone (`rust-lld`, see `.cargo/config.toml`):

```sh
cargo build --release --target aarch64-unknown-linux-musl   # or riscv64gc-unknown-linux-musl
# -> target/aarch64-unknown-linux-musl/release/shyper
```

`../tools/mkrootfs.sh` builds it and puts it into the MVM disk.
Usage: `shyper --help`, and the operations guide
([English](../doc/en/guide.md#10-shyper-cli-reference),
[русский](../doc/ru/guide.md#10-справочник-по-утилите-shyper)).
