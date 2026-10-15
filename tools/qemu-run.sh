#!/bin/bash
# cargo runner (see .cargo/config.toml): `cargo run --release --features qemu`
# boots the hypervisor in QEMU with the MVM disk vm0.img (tools/mkrootfs.sh).
#
# QEMU passes the device tree only to a raw image, so the ELF built by cargo
# is converted with llvm-objcopy from the llvm-tools rustup component.
# At most 8 GB of memory are given to the machine (QEMU_MEM).
set -e
ELF=$1
shift
TOOLS="$(rustc --print sysroot)/lib/rustlib/$(rustc -vV | sed -n 's/^host: //p')/bin"
"$TOOLS/llvm-objcopy" -O binary "$ELF" "$ELF.bin"

DISK=${DISK:-vm0.img}
MEM=${QEMU_MEM:-4g}
# GUI=1 opens a QEMU window with the MVM desktop: the MVM framebuffer
# (emulated PL111) is shown through ramfb, and a USB keyboard and tablet on
# the PCIe bus are passed through to the MVM. PCIe must stay below 4 GB.
MACHINE=virt,virtualization=on,gic-version=2
DISPLAY_ARGS=(-nographic)
if [ "${GUI:-0}" != 0 ]; then
    MACHINE=$MACHINE,highmem-ecam=off,highmem-mmio=off
    DISPLAY_ARGS=(-device ramfb -device qemu-xhci -device usb-kbd -device usb-tablet
        -display "${QEMU_DISPLAY:-gtk}" -serial mon:stdio)
fi
case "$(file -b "$ELF")" in
*aarch64*)
    exec qemu-system-aarch64 -machine "$MACHINE" -m "$MEM" -cpu cortex-a57 -smp 4 \
        "${DISPLAY_ARGS[@]}" -global virtio-mmio.force-legacy=false \
        -drive file="$DISK",if=none,format=raw,id=x0 -device virtio-blk-device,drive=x0,bus=virtio-mmio-bus.25 \
        -netdev user,id=n0,hostfwd=tcp::5555-:22 -device virtio-net-device,bus=virtio-mmio-bus.24,netdev=n0 \
        -kernel "$ELF.bin" "$@"
    ;;
*RISC-V*)
    exec qemu-system-riscv64 -machine virt -m "${QEMU_MEM:-8g}" -smp 4 -nographic -bios default \
        -drive file="$DISK",if=none,format=raw,id=x0 -device virtio-blk-device,drive=x0,bus=virtio-mmio-bus.0 \
        -netdev user,id=n0,hostfwd=tcp::5555-:22 -device virtio-net-device,netdev=n0 \
        -kernel "$ELF.bin" "$@"
    ;;
*)
    echo "unknown image $ELF" >&2
    exit 1
    ;;
esac
