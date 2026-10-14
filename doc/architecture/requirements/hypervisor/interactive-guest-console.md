---
kind: functional
origin: intent
satisfied-by: [Hypervisor.DeviceEmu, Mvm.Cli]
deferred:
---

# Interactive guest console

An operator can use a shell on a guest from the MVM.

## System Context

Guest and MVM virtio consoles are connected by the hypervisor; the MVM side is /dev/hvc0.

## Satisfy

The CLI `vm console` bridges the terminal and /dev/hvc0 in raw mode.

- test — commands typed in `shyper vm console` run in the guest
