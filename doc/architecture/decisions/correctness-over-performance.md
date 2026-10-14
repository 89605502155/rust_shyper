---
links: [deliver-guest-timer-interrupts, Hypervisor.Vgic]
prefer: [correctness, reliability]
over: [performance]
---

# Correctness over performance

Interrupt delivery and isolation must never lose an event, even if the fix
costs extra hypervisor work on the interrupt path.
