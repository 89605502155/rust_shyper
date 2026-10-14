#!/bin/bash
# All static checks of trosna-shyper, run with cargo:
#   * rustfmt of the hypervisor, the fdt crate and the CLI
#   * clippy of the hypervisor for every supported board / feature set
#   * clippy of the CLI (AArch64 and RISC-V)
#   * unit tests of the fdt crate (on the host)
# Exits non-zero if anything fails or emits a warning.
set -u
cd "$(dirname "${BASH_SOURCE[0]}")/.."

HOST=$(rustc -vV | sed -n 's/^host: //p')
failed=0

step() { # <name> <command...>
    local out st
    out=$("${@:2}" 2>&1)
    st=$?
    local warnings
    warnings=$(grep -c "^warning\|^error" <<<"$out")
    if [ $st -ne 0 ] || [ "$warnings" -ne 0 ]; then
        printf '%-58s FAIL (status %d, %d warnings/errors)\n' "$1" $st "$warnings"
        grep -E "^(warning|error)" -A6 <<<"$out" | head -40
        failed=1
    else
        printf '%-58s ok\n' "$1"
    fi
}

step "fmt: hypervisor + fdt" cargo fmt --all --check
step "fmt: cli" bash -c "cd cli && cargo fmt --check"

AARCH64=aarch64-unknown-none-softfloat
RISCV64=riscv64imac-unknown-none-elf
for features in qemu qemu,gicv3 tx2 tx2,ramdisk pi4 rk3588; do
    step "clippy: $AARCH64 $features" \
        cargo clippy -q --release --target $AARCH64 --no-default-features --features "$features"
done
for features in qemu,plic qemu,aia qemu,aia,sbi_legacy; do
    step "clippy: $RISCV64 $features" \
        cargo clippy -q --release --target $RISCV64 --no-default-features --features "$features"
done

for target in aarch64-unknown-linux-musl riscv64gc-unknown-linux-musl; do
    step "clippy: cli $target" bash -c "cd cli && cargo clippy -q --release --target $target"
done

step "test: fdt crate ($HOST)" \
    cargo test -q --manifest-path deps/libfdt-rs/Cargo.toml --target "$HOST" --target-dir target/host
step "clippy: fdt crate ($HOST)" \
    cargo clippy -q --all-targets --manifest-path deps/libfdt-rs/Cargo.toml --target "$HOST" --target-dir target/host

exit $failed
