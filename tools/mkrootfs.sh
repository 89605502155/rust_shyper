#!/bin/bash
# Build the MVM root disk (vm0.img) for `make run` on QEMU.
#
# The rootfs is the official Fedora aarch64 container image. It has no
# systemd, so a small shell script is installed as /sbin/init.
# The kernel module tools/shyper.ko and the CLI tools/shyper are copied to
# /root so the MVM can manage guest VMs.
#
# Needs: docker with arm64 emulation (qemu-user-binfmt), mkfs.ext4 and sudo
# (to keep file ownership inside the image; set SUDO="sudo -A" for askpass).
set -e

FEDORA_IMAGE=${FEDORA_IMAGE:-registry.fedoraproject.org/fedora:42}
PACKAGES=${PACKAGES:-kmod iproute procps-ng util-linux less vim-minimal}
SUDO=${SUDO:-sudo}
OUTPUT=${1:-vm0.img}
SIZE=${SIZE:-2G}

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WORK="$(mktemp -d)"
trap '$SUDO rm -rf "$WORK"' EXIT

echo "Preparing $FEDORA_IMAGE (linux/arm64) with: $PACKAGES"
CID=$(docker create --platform linux/arm64 "$FEDORA_IMAGE" \
    sh -c "dnf -y -q install --setopt=install_weak_deps=False $PACKAGES && dnf clean all")
docker start -a "$CID"
docker export "$CID" -o "$WORK/rootfs.tar"
docker rm "$CID" >/dev/null

mkdir "$WORK/rootfs"
$SUDO tar -xpf "$WORK/rootfs.tar" -C "$WORK/rootfs" --numeric-owner

$SUDO install -m 0644 "$ROOT_DIR/tools/shyper.ko" "$WORK/rootfs/root/shyper.ko"
$SUDO install -m 0755 "$ROOT_DIR/tools/shyper" "$WORK/rootfs/root/shyper"
$SUDO tee "$WORK/rootfs/usr/sbin/init" >/dev/null <<'EOF'
#!/bin/bash
mount -t proc proc /proc
mount -t sysfs sysfs /sys
mount -t devtmpfs devtmpfs /dev 2>/dev/null
mkdir -p /dev/pts /dev/shm /run /tmp
mount -t devpts devpts /dev/pts
mount -t tmpfs tmpfs /run
echo shyper-mvm > /proc/sys/kernel/hostname
. /etc/os-release
echo "MVM: $PRETTY_NAME, kernel $(uname -r) ($(uname -m))"
if insmod /root/shyper.ko; then
    echo "MVM: shyper.ko loaded"
fi
cd /root
export HOME=/root TERM=vt100 PS1='[mvm \w]# '
while true; do
    setsid -c /bin/bash -i </dev/console >/dev/console 2>&1
done
EOF
$SUDO chmod 0755 "$WORK/rootfs/usr/sbin/init"

rm -f "$OUTPUT"
truncate -s "$SIZE" "$OUTPUT"
$SUDO mkfs.ext4 -q -F -L rootfs -d "$WORK/rootfs" "$OUTPUT"
$SUDO chown "$(id -u):$(id -g)" "$OUTPUT"
echo "Created $OUTPUT"
