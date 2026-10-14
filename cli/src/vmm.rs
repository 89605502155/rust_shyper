// Copyright (c) 2023 Beihang University, Huawei Technologies Co.,Ltd. All rights reserved.
// Rust-Shyper is licensed under Mulan PSL v2.
// You can use this software according to the terms and conditions of the Mulan PSL v2.
// You may obtain a copy of Mulan PSL v2 at:
//          http://license.coscl.org.cn/MulanPSL2
// THIS SOFTWARE IS PROVIDED ON AN "AS IS" BASIS, WITHOUT WARRANTIES OF ANY KIND,
// EITHER EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO NON-INFRINGEMENT,
// MERCHANTABILITY OR FIT FOR A PARTICULAR PURPOSE.
// See the Mulan PSL v2 for more details.

use std::mem;

use log::{error, info};

use crate::{
    dev::Shyper,
    hvc::{
        generate_hvc_mode, HVC_VMM, HVC_VMM_BOOT_VM, HVC_VMM_GET_VM_ID, HVC_VMM_LIST_VM, HVC_VMM_REBOOT_VM,
        HVC_VMM_SHUTDOWN_VM, HVC_VMM_VM_REMOVE,
    },
    util::cstr_arr_to_string,
};

pub const NAME_MAX_LEN: usize = 32;
const VM_NUM_MAX: usize = 16;
const VM_PAGE_SIZE: usize = 0x1000;

#[repr(C)]
#[derive(Clone)]
struct VMInfo {
    pub id: u32,
    pub vm_name: [u8; NAME_MAX_LEN],
    pub vm_type: u32,
    pub vm_state: u32,
}

impl VMInfo {
    const EMPTY: Self = Self {
        id: 0,
        vm_name: [0; NAME_MAX_LEN],
        vm_type: 0,
        vm_state: 0,
    };
}

const VM_T_LINUX: u32 = 0;
const VM_T_BARE_MATEL_APP: u32 = 1;
const VM_T_FREERTOS: u32 = 2;

const VM_S_INV: u32 = 0;
const VM_S_PENDING: u32 = 1;
const VM_S_ACTIVE: u32 = 2;

#[repr(C)]
struct VMInfoList {
    pub vm_num: usize,
    pub info_list: [VMInfo; VM_NUM_MAX],
}

/// The module writes a whole page.
#[repr(C)]
struct VMInfoListPage {
    list: VMInfoList,
    _pad: [u8; VM_PAGE_SIZE - mem::size_of::<VMInfoList>()],
}

fn vmm_ioctl(request: usize, arg: u64) {
    if let Err(err) = Shyper::open().and_then(|dev| dev.ioctl_val(request, arg)) {
        error!("err: ioctl fail: {}", err);
    }
}

fn force_arg(force: bool, vm_id: u32) -> u64 {
    match force {
        true => ((1 << 16) | vm_id) as u64,
        false => vm_id as u64,
    }
}

pub fn vmm_boot(vm_id: u32) {
    vmm_ioctl(generate_hvc_mode(HVC_VMM, HVC_VMM_BOOT_VM), vm_id as u64);
}

pub fn vmm_shutdown(force: bool, vm_id: u32) {
    vmm_ioctl(generate_hvc_mode(HVC_VMM, HVC_VMM_SHUTDOWN_VM), force_arg(force, vm_id));
}

pub fn vmm_reboot(force: bool, vm_id: u32) {
    vmm_ioctl(generate_hvc_mode(HVC_VMM, HVC_VMM_REBOOT_VM), force_arg(force, vm_id));
}

pub fn vmm_remove(vm_id: u32) {
    vmm_ioctl(generate_hvc_mode(HVC_VMM, HVC_VMM_VM_REMOVE), vm_id as u64);
}

pub fn vmm_getvmid() {
    let mut id: u32 = 0;
    // SAFETY: the module writes the VM id as a u32.
    match Shyper::open()
        .and_then(|dev| unsafe { dev.ioctl_ref(generate_hvc_mode(HVC_VMM, HVC_VMM_GET_VM_ID), &mut id) })
    {
        Ok(()) => info!("Current VM id is {}", id),
        Err(err) => error!("err: ioctl fail: {}", err),
    }
}

pub fn vmm_list_vm_info() {
    let mut buf = Box::new(VMInfoListPage {
        list: VMInfoList {
            vm_num: 0,
            info_list: [VMInfo::EMPTY; VM_NUM_MAX],
        },
        _pad: [0; VM_PAGE_SIZE - mem::size_of::<VMInfoList>()],
    });
    // SAFETY: the module fills one page with a `VMInfoList`.
    match Shyper::open()
        .and_then(|dev| unsafe { dev.ioctl_ref(generate_hvc_mode(HVC_VMM, HVC_VMM_LIST_VM), &mut *buf) })
    {
        Ok(()) => display_vm_list_info(&buf.list),
        Err(err) => error!("err: ioctl fail: {}", err),
    }
}

fn display_vm_list_info(vm_info_list: &VMInfoList) {
    let vm_num = vm_info_list.vm_num.min(VM_NUM_MAX);
    for i in 0..vm_num {
        let info = &vm_info_list.info_list[i];
        println!("----------vm[{}]----------", i);
        println!(
            "vm id [{}] name: {} type: {}\n",
            info.id,
            cstr_arr_to_string(info.vm_name.as_slice()),
            info.vm_type
        );
        match info.vm_type {
            VM_T_LINUX => {
                println!("vm type: Linux");
            }
            VM_T_BARE_MATEL_APP => {
                println!("vm type: Bare Metal App");
            }
            VM_T_FREERTOS => {
                println!("vm type: FreeRTOS");
            }
            _ => {
                println!("vm type: illegal type");
            }
        }

        match info.vm_state {
            VM_S_INV => {
                println!("vm state: Inactive");
            }
            VM_S_PENDING => {
                println!("vm state: Pending");
            }
            VM_S_ACTIVE => {
                println!("vm state: Active");
            }
            _ => {
                println!("vm state: illegal state");
            }
        }
    }
}
