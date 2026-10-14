#!/bin/bash
# Build the MVM root disk (vm0.img) for `make run` on QEMU (aarch64).
#
# The rootfs is the official Fedora aarch64 container image. It has no
# systemd, so a small shell script is installed as /sbin/init.
#
# /root of the MVM gets everything needed to run a guest VM (GVM):
#   shyper, shyper.ko   CLI (built from cli/) and the MVM kernel module
#   gvm/Image           GVM kernel
#   gvm/gvm.img         GVM root disk, served by the CLI as a mediated disk
#   gvm/gvm.json        GVM configuration (tools/gvm-qemu.json)
#   start-gvm.sh        configures and boots the GVM, its console is /dev/hvc0
#
# Nothing is executed for arm64 on the host: the container image is only
# exported, and the kernel module is loaded by the Rust CLI instead of kmod.
# A Fedora image pulled by this script is removed again at the end.
#
# Needs: docker, mkfs.ext4, the Rust target aarch64-unknown-linux-musl and
# sudo (to keep file ownership inside the image; set SUDO="sudo -A" for
# askpass).
set -e

FEDORA_IMAGE=${FEDORA_IMAGE:-registry.fedoraproject.org/fedora:42}
OUTPUT=${1:-vm0.img}
SIZE=${SIZE:-3G}
GVM_SIZE=${GVM_SIZE:-768M}
GVM_KERNEL=${GVM_KERNEL:-image/Image_vanilla}
SUDO=${SUDO:-sudo}

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WORK="$(mktemp -d)"
PULLED=
cleanup() {
    $SUDO rm -rf "$WORK"
    if [ -n "$PULLED" ]; then
        docker rmi -f "$FEDORA_IMAGE" >/dev/null
    fi
}
trap cleanup EXIT

if ! docker image inspect "$FEDORA_IMAGE" >/dev/null 2>&1; then
    PULLED=1
fi

echo "Building the CLI..."
# cli/.cargo/config.toml selects rust-lld, it is only read from that directory.
(cd "$ROOT_DIR/cli" && cargo build -q --release --target aarch64-unknown-linux-musl)
CLI="$ROOT_DIR/cli/target/aarch64-unknown-linux-musl/release/shyper-cli-rust"

export_rootfs() { # <dir>
    local cid
    cid=$(docker create --platform linux/arm64 "$FEDORA_IMAGE")
    docker export "$cid" -o "$WORK/rootfs.tar"
    docker rm "$cid" >/dev/null
    mkdir -p "$1"
    $SUDO tar -xpf "$WORK/rootfs.tar" -C "$1" --numeric-owner
    rm "$WORK/rootfs.tar"
}

common_init() { # <hostname> <prompt>: mounts, hostname and a banner
    cat <<EOF
#!/bin/bash
mount -t proc proc /proc
mount -t sysfs sysfs /sys
mount -t devtmpfs devtmpfs /dev 2>/dev/null
mkdir -p /dev/pts /dev/shm /run /tmp
mount -t devpts devpts /dev/pts
mount -t tmpfs tmpfs /run
echo $1 > /proc/sys/kernel/hostname
. /etc/os-release
echo "$2: \$PRETTY_NAME, kernel \$(uname -r) (\$(uname -m))"
export HOME=/root TERM=vt100 PS1='[$2 \\w]# '
cd /root
EOF
}

echo "Preparing the GVM root disk..."
export_rootfs "$WORK/gvm"
{
    common_init fedora-guest gvm
    cat <<'EOF'
while true; do
    setsid -c /bin/bash -i </dev/hvc0 >/dev/hvc0 2>&1
done
EOF
} | $SUDO tee "$WORK/gvm/usr/sbin/init" >/dev/null
$SUDO chmod 0755 "$WORK/gvm/usr/sbin/init"
truncate -s "$GVM_SIZE" "$WORK/gvm.img"
$SUDO mkfs.ext4 -q -F -L gvm -d "$WORK/gvm" "$WORK/gvm.img"
$SUDO rm -rf "$WORK/gvm"
GVM_SECTORS=$(($(stat -c %s "$WORK/gvm.img") / 512))

echo "Preparing the MVM root disk..."
export_rootfs "$WORK/rootfs"
R="$WORK/rootfs/root"
$SUDO install -m 0644 "$ROOT_DIR/tools/shyper.ko" "$R/shyper.ko"
$SUDO install -m 0755 "$CLI" "$R/shyper"
$SUDO mkdir -p "$R/gvm"
$SUDO install -m 0644 "$ROOT_DIR/$GVM_KERNEL" "$R/gvm/Image"
$SUDO install -m 0644 "$WORK/gvm.img" "$R/gvm/gvm.img"
sed "s/\"@GVM_SECTORS@\"/$GVM_SECTORS/" "$ROOT_DIR/tools/gvm-qemu.json" | $SUDO tee "$R/gvm/gvm.json" >/dev/null
$SUDO install -m 0644 "$ROOT_DIR/tools/mediated-qemu.json" "$R/gvm/mediated.json"
$SUDO tee "$R/start-gvm.sh" >/dev/null <<'EOF'
#!/bin/bash
# Configure and boot the guest VM; its console is /dev/hvc0.
set -e
cd /root/gvm
/root/shyper system daemon mediated.json > /root/gvm/daemon.log 2>&1 &
sleep 3
/root/shyper vm config gvm.json
# The MVM side of the GVM console must not echo, or the guest gets its own
# output back as input.
stty -F /dev/hvc0 raw -echo
/root/shyper vm boot 1
echo "GVM started, its console is /dev/hvc0, e.g.: cat /dev/hvc0 & cat > /dev/hvc0"
EOF
$SUDO chmod 0755 "$R/start-gvm.sh"
{
    common_init shyper-mvm mvm
    cat <<'EOF'
/root/shyper system insmod /root/shyper.ko
while true; do
    setsid -c /bin/bash -i </dev/console >/dev/console 2>&1
done
EOF
} | $SUDO tee "$WORK/rootfs/usr/sbin/init" >/dev/null
$SUDO chmod 0755 "$WORK/rootfs/usr/sbin/init"

rm -f "$OUTPUT"
truncate -s "$SIZE" "$OUTPUT"
$SUDO mkfs.ext4 -q -F -L rootfs -d "$WORK/rootfs" "$OUTPUT"
$SUDO chown "$(id -u):$(id -g)" "$OUTPUT"
echo "Created $OUTPUT"
