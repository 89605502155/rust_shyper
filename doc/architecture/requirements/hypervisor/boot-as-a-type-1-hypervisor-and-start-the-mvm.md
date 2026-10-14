---
kind: functional
origin: intent
satisfied-by: [Hypervisor.Boot, Hypervisor.Vmm, Hypervisor.MvmImage, Mvm]
deferred:
---

# Boot as a type-1 hypervisor and start the MVM

The hypervisor boots at EL2 on bare hardware (or QEMU) and starts the management VM with no operator action.

## System Context

Firmware or QEMU loads the raw image; there is no host OS below the hypervisor.

## Satisfy

Boot sets up the stage-2 MMU and the cores, the VMM loads the embedded MVM kernel and a patched device tree and runs it on core 0.

- test — `cargo run --release` reaches the `[mvm ~]#` prompt of the MVM
