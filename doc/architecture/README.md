# Architecture model / Архитектурная модель

The architecture of trosna-shyper recovered from the code with
`archi` 0.1.12 (archiplan, brownfield workflow).
The spec lives in its own repository `trosna-shyper-archi` (with the
lifecycle state of archi); this folder is a copy for reading.

Архитектура trosna-shyper, восстановленная по коду инструментом `archi`
(сценарий brownfield). Спецификация ведётся в отдельном репозитории
`trosna-shyper-archi`; здесь — её копия для чтения.

- `src/model.arch` — the hypervisor and its parts / гипервизор и его части
- `src/vms.arch` — MVM (CLI, daemon, kernel module) and guest VMs / MVM и гостевые ВМ
- `requirements/hypervisor/` — intent and 8 requirements / намерение и 8 требований
- `decisions/` — trade-offs / компромиссы

## Interactions / Взаимодействия (`archi query --kind connection | archi viz`)

```
  [Gvm]   [Mvm.Cli]   [Mvm.Daemon]
    └────────┐└──────────┌──┘
             ↓           ↓
       (MmioAccess)   (Ioctl)
          ┌──┘           └┐
          ↓               ↓
    [Hypervisor]   [Mvm.ShyperKo]
        ┌─└────────┐──────────┐
        ↓          ↓          ↓
  (BlkRequest)   (Dtb)   (GuestIrq)
        └─────────┌┘          │
                  ┌───────────┘
                  ↓
                [Mvm]
                  │
                  ↓
             (Hypercall)
```

## Results / Результаты проверки

| Check | Result |
|-------|--------|
| `archi check` (model, requirements, decisions) | no findings / замечаний нет |
| `archi link verify` (model ↔ code) | 35 of 35 links clean / 35 из 35 связей чистые |
| `archi link audit` (code changes without an architectural account) | none in the code / в коде нет |
| NKP coupling | N=9, E=11, K̄=1.22, regime CRITICAL (the target regime of archi) |

The only element without code is `Mvm.ShyperKo`: the MVM kernel module is
shipped prebuilt (`tools/shyper.ko`) without sources. The highest coupling is
the `Hypervisor` node itself (K=3), expected for a monolithic hypervisor.

Единственный элемент без кода — `Mvm.ShyperKo`: модуль ядра MVM поставляется
готовым бинарником без исходников. Наибольшая связность — у узла
`Hypervisor` (K=3), что ожидаемо для монолитного гипервизора.

## Links model → code / Связи модель → код

```
Hypervisor.Boot ← hv//src/arch/aarch64/start.rs#_start
Hypervisor.Kernel ← hv//src/kernel/hvc.rs#hvc_guest_handler
Hypervisor.Vgic ← hv//src/arch/aarch64/vgic.rs#Vgic
Hypervisor.Stage2 ← hv//src/arch/aarch64/page_table.rs#PageTable
Hypervisor.DeviceEmu ← hv//src/device/emu.rs#emu_handler
Hypervisor.DeviceEmu ← hv//src/device/virtio/mmio.rs#VirtioMmio
Hypervisor.Vmm ← hv//src/vmm/manager.rs#vmm_boot_vm
Hypervisor.Vmm ← hv//src/vmm/manager.rs#vmm_init_gvm
Hypervisor.Config ← hv//src/config/config.rs#VmConfigEntry
Hypervisor.Fdt ← hv//deps/libfdt-rs/src/raw.rs#setprop
Hypervisor.MvmImage ← hv//src/vmm/image.rs#vm0_image
Mvm.Cli ← hv//cli/src/main.rs#main
Mvm.Cli ← hv//cli/src/console.rs#vm_console
Mvm.Daemon ← hv//cli/src/daemon.rs#init_daemon
Mvm.Daemon ← hv//cli/src/blk.rs#mediated_blk_read
Hypervisor.Kernel ← hv//src/kernel/sched/sched_rr.rs#needs_tick
Hypervisor.Kernel ← hv//src/kernel/sched/sched_rr.rs#SchedulerRR
Hypervisor.Kernel ← hv//src/kernel/sched/sched_rr.rs#SchedulerRR::sleep
Hypervisor.Kernel ← hv//src/kernel/sched/sched_rr.rs#SchedulerRR::wakeup
Hypervisor.Kernel ← hv//src/kernel/sched/sched_rr.rs#SchedulerRR::yield_to
Hypervisor ← hv//src/lib.rs#init
Hypervisor.DeviceEmu ← hv//src/kernel/async_task.rs#async_task_exe
Hypervisor.DeviceEmu ← hv//src/kernel/async_task.rs#async_ipi_req
Hypervisor.DeviceEmu ← hv//src/kernel/async_task.rs#finish_async_task
Hypervisor.DeviceEmu ← hv//src/kernel/async_task.rs#complete_async_task
Hypervisor.DeviceEmu ← hv//src/device/virtio/blk.rs#generate_blk_req
Hypervisor.DeviceEmu ← hv//src/device/virtio/blk.rs#virtio_mediated_blk_notify_handler
Hypervisor.Vgic ← hv//src/arch/aarch64/vgic.rs#gic_maintenance_handler
Hypervisor.Kernel ← hv//src/kernel/hvc.rs#hvc_vmm_handler
Hypervisor.Kernel ← hv//src/kernel/hvc.rs#hvc_sys_handler
Hypervisor.Kernel ← hv//src/kernel/hvc.rs#hvc_send_msg_to_vm
Hypervisor.Kernel ← hv//src/kernel/hvc.rs#hvc_ipi_handler
Mvm.Cli ← hv//cli/src/main.rs
Mvm.Cli ← hv//cli/src/vmm.rs
Mvm ← hv//tools/mkrootfs.sh
```
