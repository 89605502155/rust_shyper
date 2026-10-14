// Copyright (c) 2023 Beihang University, Huawei Technologies Co.,Ltd. All rights reserved.
// Copyright (c) 2026 Andrey Ferubko.
// trosna-shyper (based on Rust-Shyper) is licensed under Mulan PSL v2.
// You can use this software according to the terms and conditions of the Mulan PSL v2.
// You may obtain a copy of Mulan PSL v2 at:
//          http://license.coscl.org.cn/MulanPSL2
// THIS SOFTWARE IS PROVIDED ON AN "AS IS" BASIS, WITHOUT WARRANTIES OF ANY KIND,
// EITHER EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO NON-INFRINGEMENT,
// MERCHANTABILITY OR FIT FOR A PARTICULAR PURPOSE.
// See the Mulan PSL v2 for more details.

//! The helpers Rust-Shyper uses to patch guest device trees. They keep the
//! interface of the former C `wrapper.c`: a raw pointer to a blob that is
//! trusted to hold `totalsize` bytes, NUL terminated names and libfdt style
//! return codes.
//!
//! Note: like the original, these helpers assume `#address-cells = <2>`,
//! `#size-cells = <2>` and an existing `/chosen` node.

use alloc::vec::Vec;

use crate::myctypes::*;
use crate::raw::{self, FdtResult};

/// A guest memory region for [`fdt_set_memory`].
#[repr(C)]
#[allow(non_camel_case_types)]
#[derive(Debug, Clone, Copy)]
pub struct region {
    pub ipa_start: u64,
    pub length: u64,
}

const PAGE_SIZE: usize = 4096;

/// The blob at `fdt`, `totalsize` bytes long.
///
/// # Safety
/// `fdt` must point to a device tree whose `totalsize` bytes are writable.
unsafe fn blob<'a>(fdt: *mut c_void) -> &'a mut [u8] {
    let size = u32::from_be_bytes(core::ptr::read_unaligned((fdt as *const u8).add(4) as *const [u8; 4])) as usize;
    core::slice::from_raw_parts_mut(fdt as *mut u8, size)
}

/// The bytes of a NUL terminated string, without the NUL.
///
/// # Safety
/// `s` must point to a NUL terminated string.
unsafe fn cstr<'a>(s: *const c_char) -> &'a [u8] {
    let mut len = 0;
    while *s.add(len) != 0 {
        len += 1;
    }
    core::slice::from_raw_parts(s, len)
}

fn code(r: FdtResult<()>) -> c_int {
    match r {
        Ok(()) => 0,
        Err(e) => e,
    }
}

fn offset(r: FdtResult<i32>) -> c_int {
    r.unwrap_or_else(|e| e)
}

fn be32s(values: &[u32]) -> Vec<u8> {
    values.iter().flat_map(|v| v.to_be_bytes()).collect()
}

fn be64s(values: &[u64]) -> Vec<u8> {
    values.iter().flat_map(|v| v.to_be_bytes()).collect()
}

fn add_root_node(b: &mut [u8], name: &[u8]) -> FdtResult<i32> {
    raw::add_subnode(b, 0, name)
}

pub unsafe fn fdt_remove_node(fdt: *mut c_void, path: *const c_char) -> c_int {
    let b = blob(fdt);
    match raw::path_offset(b, cstr(path)) {
        Ok(node) => {
            let _ = raw::del_node(b, node);
            0
        }
        Err(_) => -1,
    }
}

pub unsafe fn fdt_disable_node(fdt: *mut c_void, path: *const c_char) -> c_int {
    let b = blob(fdt);
    let Ok(node) = raw::path_offset(b, cstr(path)) else {
        return -1;
    };
    match raw::getprop(b, node, b"status") {
        // Overwrite in place so that the blob does not need to grow; any value
        // other than "okay" disables the device.
        Ok(old) => {
            let mut val = alloc::vec![0u8; old.len()];
            let n = val.len().min(4);
            val[..n].copy_from_slice(&b"NILL"[..n]);
            let _ = raw::setprop_inplace(b, node, b"status", &val);
        }
        Err(_) => {
            let _ = raw::setprop_string(b, node, b"status", b"disabled");
        }
    }
    0
}

pub unsafe fn fdt_create_node(fdt: *mut c_void, path: *const c_char, name: *const c_char) -> c_int {
    let b = blob(fdt);
    offset(raw::path_offset(b, cstr(path)).and_then(|parent| raw::add_subnode(b, parent, cstr(name))))
}

pub unsafe fn fdt_add_property_string(fdt: *mut c_void, node: c_int, name: *const c_char, s: *const c_char) -> c_int {
    code(raw::setprop_string(blob(fdt), node, cstr(name), cstr(s)))
}

pub unsafe fn fdt_add_property_u32(fdt: *mut c_void, node: c_int, name: *const c_char, val: u32) -> c_int {
    code(raw::setprop_u32(blob(fdt), node, cstr(name), val))
}

pub unsafe fn fdt_add_property_u64_array(
    fdt: *mut c_void,
    node: c_int,
    name: *const c_char,
    values: *mut u64,
    len: c_int,
) -> c_int {
    let values = core::slice::from_raw_parts(values, len as usize);
    code(raw::setprop(blob(fdt), node, cstr(name), &be64s(values)))
}

pub unsafe fn fdt_add_property_u32_array(
    fdt: *mut c_void,
    node: c_int,
    name: *const c_char,
    values: *mut u32,
    len: c_int,
) -> c_int {
    let values = core::slice::from_raw_parts(values, len as usize);
    code(raw::setprop(blob(fdt), node, cstr(name), &be32s(values)))
}

fn add_irq_node(b: &mut [u8], name: &[u8], compatible: &[u8], spi_irq: u32) -> FdtResult<i32> {
    let node = add_root_node(b, name)?;
    raw::setprop_string(b, node, b"compatible", compatible)?;
    raw::setprop(b, node, b"interrupts", &be32s(&[0, spi_irq, 0x1]))?;
    Ok(node)
}

pub unsafe fn fdt_add_virtio(fdt: *mut c_void, name: *const c_char, spi_irq: u32, address: u64) {
    let b = blob(fdt);
    let _ = (|| {
        let node = add_root_node(b, cstr(name))?;
        raw::setprop(b, node, b"dma-coherent", &[])?;
        raw::setprop_string(b, node, b"compatible", b"virtio,mmio")?;
        raw::setprop(b, node, b"interrupts", &be32s(&[0, spi_irq, 0x1]))?;
        raw::setprop(b, node, b"reg", &be64s(&[address, 0x400]))
    })();
}

pub unsafe fn fdt_add_vm_service(fdt: *mut c_void, spi_irq: u32, address: u64, len: u64) {
    let b = blob(fdt);
    let _ = (|| {
        let node = add_irq_node(b, b"vm_service", b"shyper", spi_irq)?;
        if address != 0 && len != 0 {
            raw::setprop(b, node, b"reg", &be64s(&[address, len]))?;
        }
        FdtResult::Ok(())
    })();
}

pub unsafe fn fdt_add_vm_service_blk(fdt: *mut c_void, spi_irq: u32) {
    let _ = add_irq_node(blob(fdt), b"vm_service_blk", b"shyper_blk", spi_irq);
}

pub unsafe fn fdt_add_timer(fdt: *mut c_void, trigger_lvl: u32) {
    let b = blob(fdt);
    let _ = (|| {
        let node = add_root_node(b, b"timer")?;
        raw::setprop_string(b, node, b"compatible", b"arm,armv8-timer")?;
        let irqs: Vec<u32> = [0xd, 0xe, 0xb, 0xa]
            .iter()
            .flat_map(|&ppi| [0x1, ppi, trigger_lvl])
            .collect();
        raw::setprop(b, node, b"interrupts", &be32s(&irqs))
    })();
}

pub unsafe fn fdt_add_cpu(fdt: *mut c_void, linear_id: u64, core_id: u8, cluster_id: u8, compatible: *const c_char) {
    // Assumes that cpu-map does not exist and #address-cells = <2>.
    let b = blob(fdt);
    let name = [b'c', b'p', b'u', b'@', b'0' + linear_id as u8];
    let _ = (|| {
        let cpus = raw::path_offset(b, b"/cpus")?;
        let node = raw::add_subnode(b, cpus, &name)?;
        raw::setprop_string(b, node, b"compatible", cstr(compatible))?;
        raw::setprop_string(b, node, b"device_type", b"cpu")?;
        raw::setprop_string(b, node, b"enable-method", b"psci")?;
        raw::setprop(b, node, b"reg", &be32s(&[0, (cluster_id as u32) << 8 | core_id as u32]))
    })();
}

fn chosen(b: &[u8]) -> FdtResult<i32> {
    raw::path_offset(b, b"/chosen")
}

pub unsafe fn fdt_set_bootcmd(fdt: *mut c_void, cmdline: *const c_char) {
    let b = blob(fdt);
    let _ = chosen(b).and_then(|node| raw::setprop_string(b, node, b"bootargs", cstr(cmdline)));
}

pub unsafe fn fdt_set_initrd(fdt: *mut c_void, start: u32, end: u32) {
    // linux,initrd-start/end only have one cell.
    let b = blob(fdt);
    let _ = chosen(b).and_then(|node| {
        raw::setprop_u32(b, node, b"linux,initrd-start", start)?;
        raw::setprop_u32(b, node, b"linux,initrd-end", end)
    });
}

pub unsafe fn fdt_clear_initrd(fdt: *mut c_void) {
    let b = blob(fdt);
    if let Ok(node) = chosen(b) {
        let _ = raw::delprop(b, node, b"linux,initrd-start");
        let _ = raw::delprop(b, node, b"linux,initrd-end");
    }
}

pub unsafe fn fdt_set_memory(fdt: *mut c_void, region_num: u64, regions: *const region, node_name: *const c_char) {
    // The memory node does not need to exist.
    const FDT_MEMORY_REGION_MAX: usize = 4;
    if region_num == 0 {
        return;
    }
    let regions = core::slice::from_raw_parts(regions, (region_num as usize).min(FDT_MEMORY_REGION_MAX));
    let b = blob(fdt);
    if let Ok(existed) = raw::node_offset_by_prop_value(b, 0, b"device_type", b"memory\0") {
        if existed > 0 {
            let _ = raw::del_node(b, existed);
        }
    }
    let _ = (|| {
        let node = add_root_node(b, cstr(node_name))?;
        raw::setprop_string(b, node, b"device_type", b"memory")?;
        let reg: Vec<u64> = regions.iter().flat_map(|r| [r.ipa_start, r.length]).collect();
        raw::setprop(b, node, b"reg", &be64s(&reg))
    })();
}

pub unsafe fn fdt_setup_gic(fdt: *mut c_void, gicd_addr: u64, gicc_addr: u64, node_name: *const c_char) -> c_int {
    let b = blob(fdt);
    let node = [&b"arm,cortex-a15-gic"[..], b"arm,gic-400", b"arm,gic-v3"]
        .iter()
        .find_map(|c| raw::node_offset_by_compatible(b, 0, c).ok());
    let Some(node) = node else {
        return 0;
    };
    let reg = be64s(&[gicd_addr, 0x1000, gicc_addr, 0x2000]);
    let r = raw::setprop(b, node, b"reg", &reg);
    let _ = raw::nop_property(b, node, b"interrupts");
    if r.is_err() {
        return 0;
    }
    let _ = raw::set_name(b, node, cstr(node_name));
    1
}

pub unsafe fn fdt_setup_serial(fdt: *mut c_void, compatible: *const c_char, addr: u64, spi_irq: u32) {
    let b = blob(fdt);
    let _ = (|| {
        let node = raw::node_offset_by_compatible(b, 0, cstr(compatible))?;
        raw::setprop(b, node, b"reg", &be64s(&[addr, 0x1000]))?;
        raw::setprop(b, node, b"interrupts", &be32s(&[0, spi_irq, 0x4]))?;
        raw::setprop_string(b, node, b"status", b"okay")?;
        raw::set_name(b, node, b"serial@0")
    })();
}

pub unsafe fn fdt_set_stdout_path(fdt: *mut c_void, p: *const c_char) {
    let b = blob(fdt);
    let _ = chosen(b).and_then(|node| raw::setprop_string(b, node, b"stdout-path", cstr(p)));
}

pub unsafe fn fdt_clear_stdout_path(fdt: *mut c_void) {
    let b = blob(fdt);
    let _ = chosen(b).and_then(|node| raw::delprop(b, node, b"stdout-path"));
}

/// Grows the blob in place by at least one page.
///
/// # Safety
/// The memory after the blob must be writable up to the new size.
pub unsafe fn fdt_enlarge(fdt: *mut c_void) {
    let old = blob(fdt).to_vec();
    let new_size = old.len().next_multiple_of(PAGE_SIZE) + PAGE_SIZE;
    let dst = core::slice::from_raw_parts_mut(fdt as *mut u8, new_size);
    let _ = raw::open_into(&old, dst);
}

pub unsafe fn fdt_size(fdt: *mut c_void) -> u64 {
    blob(fdt).len() as u64
}

pub unsafe fn fdt_pack(fdt: *mut c_void) -> c_int {
    code(raw::pack(blob(fdt)))
}

pub unsafe fn fdt_del_mem_rsv(fdt: *mut c_void, n: c_int) -> c_int {
    code(raw::del_mem_rsv(blob(fdt), n as usize))
}

pub unsafe fn fdt_setup_pmu(
    fdt: *mut c_void,
    compatible: *const c_char,
    spi_irq: *const u32,
    spi_irq_len: u32,
    irq_affi: *const u32,
    irq_affi_len: u32,
) -> c_int {
    const MAX_LEN: u32 = 8;
    if spi_irq_len != irq_affi_len || spi_irq_len >= MAX_LEN {
        return -1;
    }
    let spi_irq = core::slice::from_raw_parts(spi_irq, spi_irq_len as usize);
    let irq_affi = core::slice::from_raw_parts(irq_affi, irq_affi_len as usize);
    let b = blob(fdt);
    let Ok(node) = raw::node_offset_by_compatible(b, 0, cstr(compatible)) else {
        return -1;
    };
    let _ = raw::delprop(b, node, b"interrupts");
    let _ = raw::delprop(b, node, b"interrupt-affinity");
    let irqs: Vec<u32> = spi_irq.iter().flat_map(|&irq| [0, irq, 0x4]).collect();
    code(
        raw::setprop(b, node, b"interrupts", &be32s(&irqs))
            .and_then(|_| raw::setprop(b, node, b"interrupt-affinity", &be32s(irq_affi))),
    )
}

pub unsafe fn fdt_find_node(fdt: *mut c_void, path: *const c_char) -> c_int {
    offset(raw::path_offset(blob(fdt), cstr(path)))
}

/// Returns a pointer to the value of property `prop` of the node at `path`
/// and its length in `*len`, or null.
pub unsafe fn fdt_get_property_string(
    fdt: *mut c_void,
    path: *const c_char,
    prop: *const c_char,
    len: *mut c_int,
) -> *mut c_char {
    let b = blob(fdt);
    let res = raw::path_offset(b, cstr(path)).and_then(|node| raw::getprop(b, node, cstr(prop)));
    match res {
        Ok(v) => {
            *len = v.len() as c_int;
            v.as_ptr() as *mut c_char
        }
        Err(e) => {
            *len = e;
            core::ptr::null_mut()
        }
    }
}

pub unsafe fn fdt_overlay_apply(fdt: *mut c_void, fdto: *mut c_void) -> c_int {
    code(crate::overlay::overlay_apply(blob(fdt), blob(fdto)))
}

/// # Safety
/// `buf` must be writable for `bufsize` bytes; it may be the same as `fdt`.
pub unsafe fn fdt_open_into(fdt: *const c_void, buf: *mut c_void, bufsize: c_int) -> c_int {
    let src = blob(fdt as *mut c_void).to_vec();
    let dst = core::slice::from_raw_parts_mut(buf as *mut u8, bufsize as usize);
    code(raw::open_into(&src, dst))
}
