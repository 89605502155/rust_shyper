# Hypervisor

Run several isolated Linux virtual machines on one AArch64 (or RISC-V) board
or in QEMU with a type-1 hypervisor written in Rust only: a management VM that
starts with the hypervisor and creates, boots and stops guest VMs, each guest
on its own cores and memory, with virtual disks, network and console.
The project must build with the stable Rust toolchain alone and be usable as
an ordinary hypervisor (documented commands, reproducible images, checks).
It accepts weaker performance and fewer platforms in exchange.
