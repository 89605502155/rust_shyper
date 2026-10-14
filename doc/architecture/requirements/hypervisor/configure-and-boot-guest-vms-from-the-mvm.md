---
kind: functional
origin: intent
satisfied-by: [Mvm.Cli, Mvm.ShyperKo, Hypervisor.Kernel, Hypervisor.Vmm, Hypervisor.Fdt]
deferred:
---

# Configure and boot guest VMs from the MVM

From the MVM an operator adds a guest VM from a JSON configuration, uploads its kernel and boots it.

## System Context

The CLI runs in the MVM and reaches the hypervisor only through ioctls on /dev/shyper turned into hypercalls.

## Satisfy

The CLI sends the configuration piece by piece and the kernel in huge page chunks; the VMM builds the guest and its device tree.

- test — `start-gvm.sh` boots guest VM 1 and `shyper vm list` shows it active
