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
    fs::File,
    io::{Read, Seek, SeekFrom},
    ptr::null_mut,
};

use rustix::{
    fs::{open, Mode, OFlags},
    mm::{mmap, munmap, MapFlags, ProtFlags},
    system::finit_module,
};

use crate::{dev::Shyper, util::file_size};

/// Shared memory the hypervisor reads a new image from on live update.
const UPDATE_DEV: &str = "/dev/hyper_update";
const UPDATE_AREA_SIZE: usize = 0x8000000;
/// Images made by mkimage start with a 64 byte header.
const MKIMAGE_HEADER_SIZE: u64 = 64;

fn sys_ioctl(name: &str, request: usize, arg: u64) {
    match Shyper::open() {
        Ok(dev) => {
            println!("sys {}", name);
            if let Err(err) = dev.ioctl_val(request, arg) {
                println!("err: ioctl fail: {}", err);
            }
        }
        Err(err) => println!("err: open /dev/shyper: {}", err),
    }
}

pub fn sys_reboot(force: bool) {
    sys_ioctl("reboot", 0x0000, force as u64);
}

pub fn sys_shutdown(force: bool) {
    sys_ioctl("shutdown", 0x0001, force as u64);
}

pub fn sys_test() {
    sys_ioctl("test", 0x0004, 0);
}

/// Copies the image at `path` (without its mkimage header) to the update
/// area and returns the file size.
fn update_image(path: &String) -> Result<u64, String> {
    let size = file_size(path)?;
    if size == 0 {
        return Ok(0);
    }
    let len = (size.saturating_sub(MKIMAGE_HEADER_SIZE)) as usize;
    if len > UPDATE_AREA_SIZE {
        return Err(format!("image {} is too big ({} bytes)", path, size));
    }

    let area = open(UPDATE_DEV, OFlags::RDWR | OFlags::CLOEXEC, Mode::empty())
        .map_err(|err| format!("open {}: {}", UPDATE_DEV, err))?;
    // SAFETY: a new shared mapping of the update device, unmapped below.
    let addr = unsafe {
        mmap(
            null_mut(),
            UPDATE_AREA_SIZE,
            ProtFlags::READ | ProtFlags::WRITE,
            MapFlags::SHARED | MapFlags::LOCKED,
            &area,
            0,
        )
    }
    .map_err(|err| format!("[sys_update] update image mmap failed: {}", err))?;
    // SAFETY: the mapping is UPDATE_AREA_SIZE bytes long and only used here.
    let dst = unsafe { std::slice::from_raw_parts_mut(addr as *mut u8, len) };

    let result = File::open(path)
        .and_then(|mut image| {
            image.seek(SeekFrom::Start(MKIMAGE_HEADER_SIZE))?;
            image.read_exact(dst)
        })
        .map_err(|err| format!("read {}: {}", path, err));
    // SAFETY: `dst` is not used any more.
    let _ = unsafe { munmap(addr, UPDATE_AREA_SIZE) };
    result.map(|_| size)
}

/// Loads a kernel module, like insmod(8).
pub fn sys_insmod(path: &str) {
    let result = File::open(path).and_then(|module| Ok(finit_module(&module, c"", 0)?));
    match result {
        Ok(()) => println!("module {} loaded", path),
        Err(err) => {
            println!("err: insmod {}: {}", path, err);
            std::process::exit(1);
        }
    }
}

pub fn sys_update(path: String) {
    match update_image(&path) {
        Ok(0) => println!("File {} size is 0, abort", path),
        Ok(size) => sys_ioctl("update", 0x0000, size),
        Err(err) => println!("{}", err),
    }
}
