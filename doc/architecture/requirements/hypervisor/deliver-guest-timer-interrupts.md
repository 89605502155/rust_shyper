---
kind: functional
origin: intent
satisfied-by: [Hypervisor.Vgic, Hypervisor.Kernel]
deferred:
---

# Deliver guest timer interrupts

A guest always gets its virtual timer interrupts, otherwise it stops.

## System Context

The guest virtual timer interrupt (PPI 27) is taken at EL2 and injected through a hardware-linked list register.

## Satisfy

The vGIC injects the interrupt and the guest deactivates the physical one; it must never stay active without an owner. On the QEMU board the scheduler keeps the hypervisor tick on single-vCPU cores, because QEMU does not re-evaluate the physical CPU interface after a virtual deactivation.

- test — 90 of 90 MVM boots in QEMU pass `NET: Registered protocol family 1` (12% stopped there before the fix)
