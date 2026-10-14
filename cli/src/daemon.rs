// Copyright (c) 2023 Beihang University, Huawei Technologies Co.,Ltd. All rights reserved.
// Rust-Shyper is licensed under Mulan PSL v2.
// You can use this software according to the terms and conditions of the Mulan PSL v2.
// You may obtain a copy of Mulan PSL v2 at:
//          http://license.coscl.org.cn/MulanPSL2
// THIS SOFTWARE IS PROVIDED ON AN "AS IS" BASIS, WITHOUT WARRANTIES OF ANY KIND,
// EITHER EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO NON-INFRINGEMENT,
// MERCHANTABILITY OR FIT FOR A PARTICULAR PURPOSE.
// See the Mulan PSL v2 for more details.

use std::{
    fs::{self, File},
    io::Read,
    process,
};
use serde::{Serialize, Deserialize};
use log::{debug, error, info, warn};
use signal_hook::{consts::SIGUSR1, iterator::Signals};

use crate::{
    hvc::*,
    blk::{mediated_blk_add, mediated_blk_init, mediated_blk_read, mediated_blk_write, MediatedBlkCfg, MED_BLK_LIST},
    config::copy_img_file_to_memory,
    dev::Shyper,
    ioctl_arg::{IOCTL_SYS, IOCTL_SYS_GET_KERNEL_IMG_NAME, IOCTL_SYS_GET_VMID, IOCTL_SYS_INIT_USR_PID},
    util::cstr_arr_to_string,
    vmm::vmm_boot,
};

#[derive(Serialize, Deserialize, Debug)]
struct DaemonConfig {
    mediated: Vec<String>,
}

/// The messages the kernel module passes on from the hypervisor, read from
/// /dev/shyper. They are C structs, laid out as:
///
/// * every message: `u64 hvc_fid; u64 hvc_event;`
/// * block request: `... u16 blk_id; u32 type; u64 sector; u64 count;`
/// * config request: `... u64 vm_id;`
struct HvcMsg<'a>(&'a [u8]);

impl HvcMsg<'_> {
    fn u64_at(&self, off: usize) -> u64 {
        u64::from_ne_bytes(self.0[off..off + 8].try_into().unwrap())
    }

    fn fid(&self) -> u64 {
        self.u64_at(0)
    }

    fn event(&self) -> u64 {
        self.u64_at(8)
    }

    fn blk_id(&self) -> u16 {
        u16::from_ne_bytes(self.0[16..18].try_into().unwrap())
    }

    fn blk_type(&self) -> u32 {
        u32::from_ne_bytes(self.0[20..24].try_into().unwrap())
    }

    fn blk_sector(&self) -> u64 {
        self.u64_at(24)
    }

    fn blk_count(&self) -> u64 {
        self.u64_at(32)
    }

    fn vm_id(&self) -> u64 {
        self.u64_at(16)
    }
}

// Execute the signal processing function only once
fn sig_handle_event(signal: i32) {
    // info!("Receive signal {}", signal);

    let mut file = File::open("/dev/shyper").unwrap();
    let mut buf: [u8; 256] = [0; 256];

    let n = file.read(&mut buf).unwrap();
    drop(file);

    if n == 0 {
        warn!("Lost signal {}!", signal);
    }
    let msg = HvcMsg(&buf);

    match msg.fid() as usize {
        HVC_MEDIATED if msg.event() as usize == HVC_MEDIATED_USER_NOTIFY => match msg.blk_type() {
            0 => mediated_blk_read(msg.blk_id(), msg.blk_sector(), msg.blk_count()),
            1 => mediated_blk_write(msg.blk_id(), msg.blk_sector(), msg.blk_count()),
            t => warn!("[sig_handle_event] unknown blk req type {}", t),
        },
        HVC_CONFIG if msg.event() as usize == HVC_CONFIG_UPLOAD_KERNEL_IMAGE => {
            let vm_id = msg.vm_id();
            let fd_event = generate_hvc_mode(IOCTL_SYS, IOCTL_SYS_GET_KERNEL_IMG_NAME);

            #[repr(C)]
            struct NameArg {
                vm_id: u64,
                name_addr: *mut u8,
            }

            let mut filename: [u8; 64] = [0; 64];
            let mut name_arg: NameArg = NameArg {
                vm_id,
                name_addr: filename.as_mut_ptr(),
            };

            let dev = match Shyper::open() {
                Ok(dev) => dev,
                Err(err) => {
                    warn!("sig_handle_event: open /dev/shyper failed: {}", err);
                    return;
                }
            };
            // SAFETY: the module writes the image name (at most 64 bytes)
            // to `filename`.
            if unsafe { dev.ioctl_ref(fd_event, &mut name_arg) }.is_err() {
                warn!("sig_handle_event: failed to get VM[{}] name\n", vm_id);
                return;
            }

            let img_name = cstr_arr_to_string(filename.as_slice());
            if let Err(err) = copy_img_file_to_memory(vm_id, img_name, &dev) {
                warn!("sig_handle_event: failed to copy img file to memory: {}", err);
                return;
            }
            vmm_boot(vm_id as u32);
        }
        _ => (),
    }
}

pub fn init_daemon() {
    // IGNORE: get semaphore and file_lock
    // IGNORE: create migrate fifo file
    // IGNORE: IVC Init
    let pid = process::id();
    let mut vmid: i32 = 0;
    let dev = match Shyper::open() {
        Ok(dev) => dev,
        Err(err) => {
            error!("open /dev/shyper failed: {}", err);
            return;
        }
    };
    // Set process id, in case kernel module can send signal to cli
    if let Err(err) = dev.ioctl_val_any(generate_hvc_mode(IOCTL_SYS, IOCTL_SYS_INIT_USR_PID), pid as u64) {
        error!("ioctl set pid failed: {}", err);
        return;
    }
    // Get vmid
    // SAFETY: the module writes the VM id as an int.
    if let Err(err) = unsafe { dev.ioctl_ref_any(generate_hvc_mode(IOCTL_SYS, IOCTL_SYS_GET_VMID), &mut vmid) } {
        error!("ioctl get vmid failed: {}", err);
        return;
    }
    drop(dev);

    // Init mediated block partition
    if vmid == 0 {
        info!("VM[{}] start to init blk service\n", vmid);
        mediated_blk_init();
    }
    info!("VM[{}] daemon process init success\n", vmid);

    // TODO: The signal processing at this time is not real-time, but the signal is first written to the channel in the custom handler set in signal_hook,
    // and then captured in the user mode polling
    // Consider using signal_hook's real-time signal processing to enhance real-time performance
    let mut signals = Signals::new([SIGUSR1]).unwrap();
    for signal in &mut signals {
        sig_handle_event(signal);
    }
}

pub fn config_daemon(path: String) -> Result<(), String> {
    info!("Start Shyper-cli daemon configure");
    let json_str =
        fs::read_to_string(path.clone()).map_err(|err| format!("Open json file {} err: {}", path.clone(), err))?;
    let config: DaemonConfig = serde_json::from_str(&json_str).map_err(|err| format!("Parse json err: {}", err))?;
    debug!("config is {:?}", config);

    let mut disk_cnt = 0;
    let mut disks: Vec<MediatedBlkCfg> = Vec::new();
    for disk in config.mediated {
        // Add disk, and if error happens, skip it
        match mediated_blk_add(disk_cnt, disk.clone()) {
            Ok(cfg) => {
                disk_cnt += 1;
                disks.push(cfg);
            }
            Err(err) => warn!("Add mediated disk {} failed: {}", disk, err),
        }
    }
    MED_BLK_LIST.set(disks).unwrap();

    info!("daemon configure {} mediated disk(s)", disk_cnt);
    info!("daemon configuration finished");
    Ok(())
}
