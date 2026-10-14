# Path
# please make a rootfs image by yourself
DISK = vm0.img

# Compile
ARCH ?= aarch64
PROFILE ?= release
BOARD ?= tx2
# features, separate with comma `,`
FEATURES ?=

# Toolchain
ifeq ($(ARCH), aarch64)
	RUST_TARGET := aarch64-unknown-none-softfloat
	TEXT_START := 0x40080000
	VM0_IMAGE_PATH := ./image/Image_vanilla
else ifeq ($(ARCH), riscv64)
	RUST_TARGET := riscv64imac-unknown-none-elf
	TEXT_START := 0x80200000
	VM0_IMAGE_PATH := ./image/Image-6.10-rc1
else
$(error bad arch: $(ARCH))
endif

QEMU := qemu-system-$(ARCH)

# Only the Rust toolchain is needed: rust-lld links the image, llvm-objcopy and
# llvm-objdump come from the llvm-tools rustup component (see rust-toolchain).
RUST_TOOLS := $(shell rustc --print sysroot)/lib/rustlib/$(shell rustc -vV | sed -n 's/^host: //p')/bin
LD := "$(RUST_TOOLS)/rust-lld" -flavor gnu
OBJCOPY := "$(RUST_TOOLS)/llvm-objcopy"
OBJDUMP := "$(RUST_TOOLS)/llvm-objdump"
GDB ?= gdb-multiarch

GIC_VERSION ?= 2

ifeq ($(GIC_VERSION),3)
override FEATURES += ,gicv3,
else ifneq ($(GIC_VERSION),2)
$(error Bad gic version)
endif

IRQ ?= plic
AIA_GUESTS ?= 3

ifeq ($(ARCH), riscv64)
	ifeq ($(IRQ), plic)
	FEATURES += plic
	else ifeq ($(IRQ), aia)
	FEATURES += aia
	endif
endif

TEXT_START ?= 0x83000000
VM0_IMAGE_PATH ?= ./image/L4T

RELOCATE_IMAGE=librust_shyper.a
IMAGE=rust_shyper

TARGET_DIR=target/${RUST_TARGET}/${PROFILE}

ifeq (${ARCH}, aarch64)
	QEMU_COMMON_OPTIONS = -machine virt,virtualization=on,gic-version=$(GIC_VERSION)\
		-m 8g -cpu cortex-a57 -smp 4 -display none -global virtio-mmio.force-legacy=false\
		-kernel "${TARGET_DIR}/${IMAGE}.bin"
	QEMU_NETWORK_OPTIONS = -netdev user,id=n0,hostfwd=tcp::5555-:22 -device virtio-net-device,bus=virtio-mmio-bus.24,netdev=n0
	QEMU_DISK_OPTIONS = -drive file=${DISK},if=none,format=raw,id=x0 -device virtio-blk-device,drive=x0,bus=virtio-mmio-bus.25
	MKIMAGE_ARCH = arm64
else ifeq (${ARCH}, riscv64)
# -global virtio-mmio.force-legacy=false option for disable legacy，i.e. using the latest virtio version

	ifeq ($(IRQ), plic)
		QEMU_COMMON_OPTIONS = -machine virt
	else ifeq ($(IRQ), aia)
		QEMU_COMMON_OPTIONS = -machine virt,aia=aplic-imsic,aia-guests=$(AIA_GUESTS)
	endif
	
	QEMU_COMMON_OPTIONS += -m 8g -smp 4 -display none -bios default \
		-kernel "${TARGET_DIR}/${IMAGE}.bin"
	QEMU_NETWORK_OPTIONS = -netdev user,id=n0,hostfwd=tcp::5555-:22 -device virtio-net-device,netdev=n0
	QEMU_DISK_OPTIONS = -drive file=${DISK},if=none,format=raw,id=x0 -device virtio-blk-device,drive=x0,bus=virtio-mmio-bus.0
	MKIMAGE_ARCH = riscv
else
$(error bad qemu arch: $(ARCH))
endif

# Combine board(tx2, qemu, pi4, ...) with previous features as cargo's features
CARGO_FLAGS ?= --target ${RUST_TARGET} --no-default-features --features "${BOARD},${FEATURES}"
ifeq (${PROFILE}, release)
CARGO_FLAGS := ${CARGO_FLAGS} --release
endif

export BOARD VM0_IMAGE_PATH AIA_GUESTS = $(AIA_GUESTS)

CARGO_ACTION ?= build

TFTP_SERVER ?= root@192.168.106.153:/tftp

UBOOT_IMAGE ?= Image$(USER)_$(ARCH)_$(BOARD)

.PHONY: build doc cargo upload qemu rk3588 tx2 pi4 tx2_update tx2_ramdisk gdb clean clippy

build: cargo
	${LD} "${TARGET_DIR}/${RELOCATE_IMAGE}" -T "linkers/${ARCH}.ld" \
		--defsym TEXT_START=${TEXT_START} -o "${TARGET_DIR}/${IMAGE}"
	${OBJDUMP} --demangle -d "${TARGET_DIR}/${IMAGE}" > "${TARGET_DIR}/t.txt"
	${OBJCOPY} "${TARGET_DIR}/${IMAGE}" -O binary "${TARGET_DIR}/${IMAGE}.bin"

cargo:
	cargo ${CARGO_ACTION} ${CARGO_FLAGS}

# TODO: fix the mkimage ARCH because it only accept "arm64" and "AArch64" for aarch64
upload: build
	@mkimage -n ${IMAGE} -A ${MKIMAGE_ARCH} -O linux -T kernel -C none -a $(TEXT_START) -e $(TEXT_START) -d ${TARGET_DIR}/${IMAGE}.bin ${TARGET_DIR}/${UBOOT_IMAGE}
	@echo "*** Upload Image ${UBOOT_IMAGE} ***"
	@scp ${TARGET_DIR}/${UBOOT_IMAGE} ${TFTP_SERVER}/${UBOOT_IMAGE}

qemu:
	$(MAKE) build BOARD=qemu TEXT_START=${TEXT_START} VM0_IMAGE_PATH=${VM0_IMAGE_PATH}
	${OBJCOPY} "${TARGET_DIR}/${IMAGE}" -O binary "${TARGET_DIR}/${IMAGE}.bin"

qemu_uboot:
	$(MAKE) upload BOARD=qemu TEXT_START=0x80200000 VM0_IMAGE_PATH=./image/Image_vanilla

rk3588:
	$(MAKE) upload BOARD=rk3588 TEXT_START=0x00480000 VM0_IMAGE_PATH=./image/Image-5.10.160

tx2:
	$(MAKE) upload BOARD=tx2 TEXT_START=0x83000000 VM0_IMAGE_PATH=./image/L4T

tx2_ramdisk:
	$(MAKE) upload BOARD=tx2 FEATURES=ramdisk TEXT_START=0x83000000 VM0_IMAGE_PATH=./image/L4T

pi4:
	$(MAKE) upload BOARD=pi4 TEXT_START=0xf0080000 VM0_IMAGE_PATH=./image/Image_pi4_5.4.83_tlb


QEMU_SERIAL_OPTIONS = -serial mon:stdio #\
	-serial telnet:localhost:12345,server

# QEMU_NETWORK_OPTIONS = -netdev tap,id=tap0,ifname=tap0,script=no,downscript=no -device virtio-net-device,bus=virtio-mmio-bus.24,netdev=tap0
#/home/cwm/c-hyper/syberx-hypervisor/build/shyper_qemuv3.bin


run: qemu
	${QEMU} ${QEMU_COMMON_OPTIONS} ${QEMU_SERIAL_OPTIONS} ${QEMU_NETWORK_OPTIONS} ${QEMU_DISK_OPTIONS}

debug: qemu
	${QEMU} ${QEMU_COMMON_OPTIONS} ${QEMU_SERIAL_OPTIONS} ${QEMU_NETWORK_OPTIONS} ${QEMU_DISK_OPTIONS} \
		-s -S

gdb:
	${GDB} -x gdb/$(ARCH).gdb

# target remote localhost:1234
gdb-connect:
	echo "symbol-file ${TARGET_DIR}/${IMAGE}" > .gdbinit
	echo "target remote localhost:1234" >> .gdbinit
	gdb-multiarch -command=.gdbinit


clean:
	cargo clean

clippy: CARGO_ACTION = clippy
clippy: cargo

doc:
	$(MAKE) cargo CARGO_ACTION=doc FEATURES="doc,$(FEATURES)"
