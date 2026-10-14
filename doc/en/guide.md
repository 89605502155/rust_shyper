# trosna-shyper operations guide

[Русская версия](../ru/guide.md)

This guide explains how to build trosna-shyper, run it in QEMU, work with the
management VM and start guest virtual machines. It also covers configuration,
the command line tool, checks and troubleshooting.

Contents:

1. [What trosna-shyper is](#1-what-trosna-shyper-is)
2. [Host requirements](#2-host-requirements)
3. [Getting the sources](#3-getting-the-sources)
4. [Building](#4-building)
5. [The MVM disk](#5-the-mvm-disk)
6. [Running in QEMU](#6-running-in-qemu)
7. [Working in the MVM](#7-working-in-the-mvm)
8. [Guest VMs](#8-guest-vms)
9. [VM configuration reference](#9-vm-configuration-reference)
10. [`shyper` CLI reference](#10-shyper-cli-reference)
11. [Other boards and RISC-V](#11-other-boards-and-risc-v)
12. [Checks and tests](#12-checks-and-tests)
13. [Debugging](#13-debugging)
14. [Troubleshooting and known issues](#14-troubleshooting-and-known-issues)
15. [Differences from Rust-Shyper](#15-differences-from-rust-shyper)
16. [Repository layout](#16-repository-layout)

## 1. What trosna-shyper is

trosna-shyper is a **type-1 (bare-metal) hypervisor** written in Rust. It
runs directly on the hardware, at EL2 on AArch64 or in HS-mode on RISC-V,
with no host operating system below it. It is based on
[Rust-Shyper](https://github.com/openeuler-mirror/rust_shyper) by Beihang
University and Huawei, and it is built with stable Rust only.

It runs two kinds of virtual machines:

- **MVM (management VM)**, VM 0. It starts automatically together with the
  hypervisor. Its Linux kernel is embedded in the hypervisor image. It owns
  most physical devices and manages the other VMs with the `shyper` tool
  through the `shyper.ko` kernel module.
- **GVM (guest VMs)**, VM 1 and up. The MVM configures and boots them.
  Each GVM gets its own physical CPU cores and memory, an emulated
  interrupt controller and virtio devices:
  - a block device served by the MVM ("mediated" disk);
  - a network device;
  - a console.

  It can also get passthrough devices.

Isolation relies on hardware virtualization: stage-2 address translation
for memory, and a virtual GIC/PLIC for interrupts.

Guests must be built for the same architecture as the hypervisor (AArch64 or
RISC-V). The tested guest OS is Linux; the MVM and GVM userland used below is
Fedora 42 for AArch64.

## 2. Host requirements

The host is the machine you build on and run QEMU on (Linux, x86_64 or
AArch64).

| What | Why | Example (Ubuntu / Debian) |
|------|-----|---------------------------|
| [rustup](https://rustup.rs) | Rust toolchain. The exact version (Rust 1.99), the targets and the components are installed automatically from `rust-toolchain`. | see rustup.rs |
| `qemu-system-aarch64` | runs the hypervisor | `apt install qemu-system-arm` |
| `qemu-system-riscv64` (optional) | RISC-V | `apt install qemu-system-misc` |
| docker, `mkfs.ext4`, sudo | only to build the MVM disk (`tools/mkrootfs.sh`) | `apt install docker.io e2fsprogs` |

No C compiler, cross toolchain, binutils, libclang or `libfdisk` is needed.
The hypervisor, its libraries and the CLI are Rust only. Linking and binary
conversion use `rust-lld` and `llvm-objcopy` from rustup.

Paths with spaces or non-ASCII characters (for example `~/Рабочий стол`) are
supported.

## 3. Getting the sources

```sh
git clone <repository URL> trosna-shyper
cd trosna-shyper
```

The first cargo command installs the toolchain from `rust-toolchain`:
Rust 1.99, the targets `aarch64-unknown-none-softfloat`,
`riscv64imac-unknown-none-elf`, `aarch64-unknown-linux-musl` and
`riscv64gc-unknown-linux-musl`, and the components `rust-src`,
`llvm-tools`, `clippy` and `rustfmt`.

## 4. Building

### Hypervisor (QEMU, AArch64)

```sh
cargo build --release
```

This produces `target/aarch64-unknown-none-softfloat/release/trosna-shyper`,
an ELF image with the MVM kernel embedded. By default the embedded kernel is
`image/Image_vanilla` (Linux 4.9.140). Use another one with
`VM0_IMAGE_PATH=path/to/Image cargo build --release`.

The default board is `qemu`. Other boards are selected with features, see
[section 11](#11-other-boards-and-risc-v).

### With make

The Makefile does the same and also produces a raw binary (`.bin`) for boot
loaders:

```sh
make qemu      # target/aarch64-unknown-none-softfloat/release/trosna-shyper.bin
make run       # build and start QEMU (8 GiB)
make tx2       # / pi4 / rk3588: board images (the upload step needs mkimage and a TFTP server)
```

### CLI (`shyper`)

The CLI runs inside the MVM. It is cross compiled to a static Linux
executable, linked with `rust-lld`:

```sh
cd cli
cargo build --release --target aarch64-unknown-linux-musl
# -> cli/target/aarch64-unknown-linux-musl/release/shyper
```

`tools/mkrootfs.sh` builds it automatically.

## 5. The MVM disk

The MVM kernel mounts its root file system from `vm0.img` (virtio block
device `/dev/vda`). Build it with:

```sh
./tools/mkrootfs.sh            # creates vm0.img (3 GiB)
```

The script:

- exports the official `registry.fedoraproject.org/fedora:42` image for
  `linux/arm64` with docker. Nothing is executed for arm64 on the host,
  and the image is removed again if the script pulled it.
- installs a small `/sbin/init`. It mounts `/proc`, `/sys` and `/dev`, loads
  `shyper.ko` and starts a root shell on the console.
- puts into `/root`:
  - `shyper` — the CLI built from `cli/`;
  - `shyper.ko` — the MVM kernel module (prebuilt for Linux 4.9.140, from
    `tools/`);
  - `gvm/Image` — the guest kernel;
  - `gvm/gvm.img` — the guest root disk, Fedora 42 with an init that runs
    a shell on `/dev/hvc0`;
  - `gvm/gvm.json` and `gvm/mediated.json` — the guest configuration (from
    `tools/gvm-qemu.json` and `tools/mediated-qemu.json`);
  - `start-gvm.sh` — configures and boots the guest.

Variables: `SIZE` (MVM disk size, default `3G`), `GVM_SIZE` (default `768M`),
`GVM_KERNEL` (default `image/Image_vanilla`), `FEDORA_IMAGE`, `SUDO` (for
example `SUDO="sudo -A"`).

## 6. Running in QEMU

```sh
cargo run --release
```

`cargo run` builds the hypervisor and starts it through `tools/qemu-run.sh`:

- machine `virt` with virtualization enabled (EL2) and GICv2;
- 4 cores (Cortex-A57) and 4 GiB of memory;
- disk `vm0.img`;
- user-mode network, with host port 5555 forwarded to port 22 of the MVM.

The serial console is in your terminal.

- `QEMU_MEM=6g cargo run --release` changes the memory size. Do not use
  more than 8 GiB on a small host.
- `DISK=other.img cargo run --release` uses another disk.
- Extra arguments go to QEMU: `cargo run --release -- -s -S` (gdb).

**Leave QEMU** with `Ctrl-A` then `X`.

What you see on boot:

1. `Welcome to qemu trosna-shyper 0.2.0!`, the build information and the
   hypervisor log (`[I]>[core N, ...]`). The memory, the 4 cores, the
   interrupt controller and the timer are initialized.
2. `Start booting Monitor VM ...`, then the Linux kernel log of the MVM.
   The hypervisor log continues on the same console, colored, for example
   when a virtio device is configured.
3. `mvm: Fedora Linux 42 (Container Image), kernel 4.9.140 (aarch64)` and
   the prompt `[mvm ~]#`.

Booting takes about 1–2 minutes, because QEMU emulates the AArch64 CPUs.

## 7. Working in the MVM

The MVM is a normal Linux system with a root shell. It also has the
hypervisor tools:

```sh
cat /etc/os-release
ls -l /dev/shyper          # device of the hypervisor kernel module
./shyper --help
./shyper vm list           # VM 0 "supervisor" is the MVM itself
./shyper vm getvmid
```

`/root/shyper` and `/root/shyper.ko` are the tools; `/root/gvm` holds the guest.

## 8. Guest VMs

### Quick start

```sh
/root/start-gvm.sh
/root/shyper vm console        # guest console, Ctrl-] goes back to the MVM
```

In the guest console press Enter. The prompt `[gvm ~]#` belongs to the guest
VM (hostname `fedora-guest`):

```sh
uname -a        # Linux fedora-guest 4.9.140 ... aarch64
cat /etc/os-release
df -h /         # /dev/root: the mediated disk gvm.img
nproc           # 1: the guest has one physical core of its own
```

### Step by step

`start-gvm.sh` does the following:

```sh
cd /root/gvm
# 1. The daemon serves mediated disks for the guests and handles requests of
#    the hypervisor (it keeps running).
/root/shyper system daemon mediated.json > daemon.log 2>&1 &
# 2. Send the guest configuration and kernel to the hypervisor. The new VM
#    gets id 1.
/root/shyper vm config gvm.json
# 3. Start it on its CPU (core 2 in gvm.json).
/root/shyper vm boot 1
```

`/root/shyper vm console` connects to the guest console (`/dev/hvc0` in the
MVM). It switches the device and your terminal to raw mode.

### Managing guests

```sh
/root/shyper vm list               # VMs and their state
/root/shyper vm reboot 1 [-f]      # reboot a guest
/root/shyper vm remove 1           # stop and remove a guest (its configuration too)
/root/shyper vm delconfig 1        # drop a configuration that was not booted
```

### Your own guest

1. Copy a Linux kernel `Image` for AArch64 to the MVM. It needs virtio-mmio
   block, net and console drivers built in.
2. Make a root disk (any ext4 image or block device) and list it in
   `mediated.json`. The first entry is mediated disk 0, the next one disk 1,
   and so on.
3. Edit `gvm.json`:
   - `kernel_filename`;
   - `cmdline` (console `hvc0`, root `/dev/vda`);
   - memory;
   - CPU bitmap;
   - in `cfg_list` of `EMU_DEVICE_T_VIRTIO_BLK_MEDIATED`: the disk index and
     its size in 512-byte sectors.
4. Restart the daemon if `mediated.json` changed, then `vm config` and
   `vm boot`.

## 9. VM configuration reference

Example: `tools/gvm-qemu.json`. Numbers can be decimal or strings in hex
(`"0x80000000"`) or binary (`"0b0100"`).

| Field | Meaning |
|-------|---------|
| `name` | VM name (shown by `vm list`) |
| `type` | `VM_T_LINUX`, `VM_T_BARE_MATEL_APP` or `VM_T_FREERTOS` |
| `cmdline` | kernel command line |
| `image.kernel_filename` | kernel file, relative to the directory the CLI runs in (at most 31 characters) |
| `image.kernel_load_ipa` | guest physical address of the kernel |
| `image.kernel_entry_point` | entry point (informational, the load address is used) |
| `image.device_tree_filename` | own device tree blob, or `"-"` to let the hypervisor generate it |
| `image.device_tree_load_ipa` | address of the device tree |
| `image.ramdisk_filename`, `ramdisk_load_ipa` | initrd: not uploaded by the CLI yet, use `"-"` and `0` |
| `memory.region[]` | `ipa_start` and `length` of each guest memory region |
| `memory.colors`, `memory.budget` | optional cache coloring (`"0-13,15"`) and memory bandwidth budget |
| `cpu.num`, `cpu.allocate_bitmap`, `cpu.master` | number of vCPUs, bitmap of the physical cores (core 0 belongs to the MVM), boot core |
| `emulated_device.emulated_device_list[]` | emulated devices: `name`, `base_ipa`, `length`, `irq_id`, `cfg_list`, `type` |
| `passthrough_device.passthrough_device_list[]` | physical regions mapped into the guest: `base_pa`, `base_ipa`, `length`, `irq_list`, optional `smmu_id` |
| `dtb_device.dtb_device_list[]` | nodes put into the generated device tree (interrupt controller etc.) |

Emulated device types (`type`), with `cfg_list` where it matters:

| Type | Device | `cfg_list` |
|------|--------|------------|
| `EMU_DEVICE_T_GICD`, `EMU_DEVICE_T_GICR` | virtual GIC distributor / redistributor | – |
| `EMU_DEVICE_T_PLIC`, `EMU_DEVICE_T_APLIC` | virtual interrupt controller (RISC-V) | – |
| `EMU_DEVICE_T_VIRTIO_BLK_MEDIATED` | virtio block, served by the MVM daemon | `[disk index, size in sectors]` |
| `EMU_DEVICE_T_VIRTIO_BLK` | virtio block without a backend: not supported, its requests are dropped with a warning | – |
| `EMU_DEVICE_T_VIRTIO_NET` | virtio network, switched between VMs by the hypervisor | MAC address, 6 bytes |
| `EMU_DEVICE_T_VIRTIO_CONSOLE` | virtio console connected to another VM | `[peer VM id, peer console address]` |

Mediated disks (`mediated.json`):

```json
{ "mediated": ["/root/gvm/gvm.img", "/dev/sda2"] }
```

The MVM itself is configured in the hypervisor source, `src/config/<board>_def.rs`
(for QEMU: `src/config/qemu_def.rs`). That includes its memory, cores,
devices and kernel command line.

## 10. `shyper` CLI reference

```
shyper system daemon [mediated.json]   start the daemon (mediated disks, image uploads)
shyper system insmod [shyper.ko]       load the kernel module (no kmod needed)
shyper system reboot [-f]              reboot the hypervisor
shyper system shutdown [-f]            shut the hypervisor down
shyper system update <image>           live update (not supported by trosna-shyper yet)
shyper system test                     test hypercall
shyper vm config <vm.json>             add a VM, prints its id
shyper vm boot <id>                    start a VM
shyper vm console [/dev/hvc0]          attach to a guest console, Ctrl-] leaves
shyper vm list                         list VMs
shyper vm reboot <id> [-f]             reboot a VM
shyper vm remove <id>                  stop and remove a VM
shyper vm delconfig <id>               remove the configuration of a VM
shyper vm getvmid                      id of the VM the command runs in
```

The CLI talks to the hypervisor through `/dev/shyper` (ioctl, the kernel
module turns them into hypercalls). It uses no C library calls directly, only
Linux system calls through `rustix`.

## 11. Other boards and RISC-V

Boards are cargo features. Build without the default `qemu` feature:

```sh
cargo build --release --no-default-features --features tx2      # NVIDIA Jetson TX2
cargo build --release --no-default-features --features pi4      # Raspberry Pi 4
cargo build --release --no-default-features --features rk3588   # Firefly ROC-RK3588S-PC
cargo build --release --no-default-features --features qemu,gicv3
```

`make tx2` / `make pi4` / `make rk3588` also set the matching load address
and MVM image, and make a U-Boot image (`upload` target). These boards build
and pass the checks, but in this project only QEMU was run.

RISC-V (QEMU, PLIC or AIA interrupt controller):

```sh
cargo build --release --target riscv64imac-unknown-none-elf --no-default-features --features qemu,plic
ARCH=riscv64 make qemu          # same through make
```

For RISC-V the MVM kernel is `image/Image-6.10-rc1` and QEMU needs 8 GiB
(`cargo run --target riscv64imac-unknown-none-elf ...`). A RISC-V MVM disk
and kernel module are described in
[the upstream guide](../upstream/RISC-V64_User_Guide.md).

## 12. Checks and tests

```sh
./tools/check.sh
```

It runs:

- `cargo fmt --check`;
- `cargo clippy` for all 9 board/feature combinations of the hypervisor;
- `cargo clippy` for the CLI on AArch64 and RISC-V;
- the unit tests of the `fdt` crate (device tree editing and overlays).

It fails on any warning.

Single checks:

```sh
cargo fmt --all --check
cargo clippy --release
cargo test --manifest-path deps/libfdt-rs/Cargo.toml --target x86_64-unknown-linux-gnu
```

## 13. Debugging

- Hypervisor log: the colored `[E]/[W]/[I]` lines on the serial console.
- QEMU monitor: when QEMU runs with `-nographic`, `Ctrl-A` then `C` toggles
  between the console and the monitor. `info registers -a` shows every
  core: EL1 means a guest is running, EL2 means the hypervisor.
- gdb: `cargo run --release -- -s -S` waits for a debugger on port 1234,
  then `gdb-multiarch -x gdb/aarch64.gdb`. `make debug` / `make gdb` do the
  same through make.

## 14. Troubleshooting and known issues

| Symptom | Cause / fix |
|---------|-------------|
| `no valid device tree from the boot loader` | QEMU passes the device tree only to a raw image. Use `cargo run`, `make run` or the `.bin`, not the ELF directly. |
| `VFS: Unable to mount root fs` in the MVM | `vm0.img` is missing or not a disk image: run `tools/mkrootfs.sh`. |
| The guest console repeats its own output | the console device was used without raw mode: use `shyper vm console`. |
| `vm config` fails with `failed to copy kernel image` | `kernel_filename` is wrong, or there are no free huge pages (the MVM command line reserves 4 × 32 MiB). |
| Docker permission denied in `mkrootfs.sh` | your user is not in the `docker` group. Run the script with sudo rights or add the user to the group. |
| Very slow boot | QEMU emulates the CPUs (TCG). Close other heavy programs. |

### Note: guest interrupts under QEMU

QEMU's GICv2 model (checked up to 10.2) has a bug. When a guest deactivates
a hardware-linked interrupt through its virtual CPU interface, QEMU does not
re-evaluate the physical CPU interface. A guest timer interrupt that is
already pending again at that moment stays unsignalled until some other GIC
event happens. In Rust-Shyper this made about one MVM boot in 8 stop forever
after `NET: Registered protocol family 1`.

trosna-shyper works around it on the `qemu` board only: the hypervisor timer
also ticks on cores with a single vCPU (every 10 ms), and every tick makes
QEMU re-evaluate. On real boards the timer stays off for a single vCPU, as
before. Measured: 0 hangs in 90 boots with the workaround, about 12% without.

## 15. Differences from Rust-Shyper

Compared with upstream commit `62a2053`:

- **Builds again.** Paths with spaces are handled. The build no longer
  depends on the host glibc headers.
- **Stable Rust 1.99.** Upstream needed nightly-2023-11-09:
  - no `#![feature]` is used;
  - the built-in targets replace custom target JSON and `-Z build-std`;
  - `naked_asm!` is used for naked functions;
  - inline-asm helpers replace the RISC-V H-extension intrinsics.
- **No C or FFI:**
  - the C libfdt (bindgen + cc) was ported to a Rust crate,
    `deps/libfdt-rs`;
  - `vm-fdt` comes from crates.io instead of an old git fork;
  - the CLI uses `rustix` instead of `libc` and no longer needs `libfdisk`.
- **Only the Rust toolchain.** `rust-lld` and llvm-tools replace the GNU
  cross toolchain. The MVM image is embedded with `.incbin`.
  `cargo build`/`cargo run` work without the Makefile.
- **Bug fixes:**
  - VM task removal kept exactly the tasks it had to remove;
  - `sys update` sent the reboot hypercall instead of update;
  - images larger than 32 MiB were uploaded past the buffer;
  - the device tree file `"-"` was treated as a file name;
  - undefined behavior in the CLI: dangling pointer, unterminated strings,
    write to an immutable value;
  - a null reference at the hypervisor entry;
  - a race in the asynchronous task executor that panicked the hypervisor
    when a guest disk request was handled on two cores at once;
  - unimplemented hypercalls and non-mediated disk requests panicked the
    hypervisor; they now return an error;
  - libfdt left uninitialized data in buffers;
  - the MVM randomly stopped during boot under QEMU (a lost guest timer
    interrupt, see section 14).
- **New:**
  - `vm delconfig`, `vm console` and `system insmod` in the CLI;
  - an MVM disk with a ready guest (`tools/mkrootfs.sh`);
  - `tools/check.sh`.
- **Clean code:** `clippy` and `rustfmt` report no warnings on all boards.

## 16. Repository layout

```
src/                hypervisor (arch/, kernel/, device/, vmm/, board/, config/, ...)
src/main.rs         bootable image (cargo build / cargo run)
deps/libfdt-rs/     device tree editing in Rust (port of libfdt)
cli/                shyper, the management tool for the MVM
linkers/            linker scripts
image/              MVM kernel images
tools/              mkrootfs.sh, qemu-run.sh, check.sh, guest configuration, shyper.ko
doc/                this documentation; doc/upstream: the Rust-Shyper docs
```
