---
kind: functional
origin: intent
satisfied-by: [Hypervisor.DeviceEmu, Mvm.Daemon]
deferred:
---

# Serve guest disks from the MVM

A guest disk is a virtio block device whose requests are served by the MVM from an image file or a block device.

## System Context

Mediated disks are listed in mediated.json and attached to guests by index.

## Satisfy

The hypervisor forwards each virtio request to the MVM daemon, which reads or writes the image through a shared huge page and acknowledges it.

- test — the guest mounts its ext4 root from gvm.img (`df -h /`)
