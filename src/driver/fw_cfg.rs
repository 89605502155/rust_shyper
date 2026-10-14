// Copyright (c) 2026 Andrey Ferubko.
// trosna-shyper is licensed under Mulan PSL v2.
// You can use this software according to the terms and conditions of the Mulan PSL v2.
// You may obtain a copy of Mulan PSL v2 at:
//          http://license.coscl.org.cn/MulanPSL2
// THIS SOFTWARE IS PROVIDED ON AN "AS IS" BASIS, WITHOUT WARRANTIES OF ANY KIND,
// EITHER EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO NON-INFRINGEMENT,
// MERCHANTABILITY OR FIT FOR A PARTICULAR PURPOSE.
// See the Mulan PSL v2 for more details.

//! QEMU firmware configuration device (fw_cfg), memory mapped interface of
//! the `virt` machine. Used to set up the ramfb display.

use core::ptr::{read_volatile, write_volatile};
use core::sync::atomic::{fence, Ordering};

use spin::Mutex;

/// fw_cfg of the QEMU `virt` machine.
const FW_CFG_BASE: usize = 0x0902_0000;
const DATA: usize = FW_CFG_BASE;
const SELECTOR: usize = FW_CFG_BASE + 0x8;
const DMA: usize = FW_CFG_BASE + 0x10;

const FW_CFG_SIGNATURE: u16 = 0x00;
const FW_CFG_ID: u16 = 0x01;
const FW_CFG_FILE_DIR: u16 = 0x19;
const FW_CFG_VERSION_DMA: u32 = 0x2;

const DMA_CTL_ERROR: u32 = 0x01;
const DMA_CTL_SELECT: u32 = 0x08;
const DMA_CTL_WRITE: u32 = 0x10;

/// A DMA access descriptor, all fields big-endian.
#[repr(C, align(16))]
struct DmaAccess {
    control: u32,
    length: u32,
    address: u64,
}

/// The descriptor and the data written by DMA live here; the hypervisor maps
/// its memory 1:1, so their virtual address is the physical one QEMU needs.
struct DmaArea {
    access: DmaAccess,
    data: [u8; 64],
}

static DMA_AREA: Mutex<DmaArea> = Mutex::new(DmaArea {
    access: DmaAccess {
        control: 0,
        length: 0,
        address: 0,
    },
    data: [0; 64],
});

fn select(key: u16) {
    // SAFETY: the selector register of the fw_cfg device.
    unsafe { write_volatile(SELECTOR as *mut u16, key.to_be()) }
}

fn read_u8() -> u8 {
    // SAFETY: the data register of the fw_cfg device.
    unsafe { read_volatile(DATA as *const u8) }
}

fn read_bytes(buf: &mut [u8]) {
    for b in buf {
        *b = read_u8();
    }
}

fn read_be32() -> u32 {
    let mut b = [0u8; 4];
    read_bytes(&mut b);
    u32::from_be_bytes(b)
}

/// Whether a QEMU fw_cfg device with the DMA interface is present.
pub fn fw_cfg_present() -> bool {
    select(FW_CFG_SIGNATURE);
    let mut sig = [0u8; 4];
    read_bytes(&mut sig);
    if &sig != b"QEMU" {
        return false;
    }
    select(FW_CFG_ID);
    let mut id = [0u8; 4];
    read_bytes(&mut id);
    u32::from_le_bytes(id) & FW_CFG_VERSION_DMA != 0
}

/// The selector of the fw_cfg file `name`, if QEMU provides it.
pub fn fw_cfg_find_file(name: &str) -> Option<u16> {
    if !fw_cfg_present() {
        return None;
    }
    select(FW_CFG_FILE_DIR);
    let count = read_be32();
    for _ in 0..count {
        let _size = read_be32();
        let mut sel = [0u8; 2];
        read_bytes(&mut sel);
        let mut reserved = [0u8; 2];
        read_bytes(&mut reserved);
        let mut file = [0u8; 56];
        read_bytes(&mut file);
        let len = file.iter().position(|&c| c == 0).unwrap_or(file.len());
        if &file[..len] == name.as_bytes() {
            return Some(u16::from_be_bytes(sel));
        }
    }
    None
}

/// Writes `data` (at most 64 bytes) to the fw_cfg item `selector` by DMA.
pub fn fw_cfg_write(selector: u16, data: &[u8]) -> Result<(), ()> {
    let mut area = DMA_AREA.lock();
    if data.len() > area.data.len() {
        return Err(());
    }
    area.data[..data.len()].copy_from_slice(data);
    let data_addr = area.data.as_ptr() as u64;
    area.access = DmaAccess {
        control: ((selector as u32) << 16 | DMA_CTL_SELECT | DMA_CTL_WRITE).to_be(),
        length: (data.len() as u32).to_be(),
        address: data_addr.to_be(),
    };
    let access_addr = &area.access as *const DmaAccess as u64;
    fence(Ordering::SeqCst);
    // SAFETY: the DMA register of the fw_cfg device; QEMU runs the transfer
    // synchronously during this write.
    unsafe { write_volatile(DMA as *mut u64, access_addr.to_be()) };
    fence(Ordering::SeqCst);
    // SAFETY: the descriptor is updated by QEMU.
    let control = u32::from_be(unsafe { read_volatile(&area.access.control) });
    if control & DMA_CTL_ERROR != 0 {
        return Err(());
    }
    Ok(())
}
