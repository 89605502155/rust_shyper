---
kind: functional
origin: intent
satisfied-by: [Hypervisor.Stage2, Hypervisor.Kernel]
deferred:
---

# Isolate guest cores and memory

Each guest VM runs on its own physical cores and can only access the memory assigned to it.

## System Context

The MVM owns core 0; guests take the cores of their allocation bitmap.

## Satisfy

Stage-2 translation maps only the VM regions, the scheduler pins vCPUs to the allocated cores.

- test — the guest reports one CPU (`nproc`) and only its configured memory
