// Copyright (c) 2023 Beihang University, Huawei Technologies Co.,Ltd. All rights reserved.
// Rust-Shyper is licensed under Mulan PSL v2.
// You can use this software according to the terms and conditions of the Mulan PSL v2.
// You may obtain a copy of Mulan PSL v2 at:
//          http://license.coscl.org.cn/MulanPSL2
// THIS SOFTWARE IS PROVIDED ON AN "AS IS" BASIS, WITHOUT WARRANTIES OF ANY KIND,
// EITHER EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO NON-INFRINGEMENT,
// MERCHANTABILITY OR FIT FOR A PARTICULAR PURPOSE.
// See the Mulan PSL v2 for more details.

//! Hypervisor extension instructions. `core::arch::riscv64` has them only
//! behind the unstable `riscv_ext_intrinsics` feature, so they are encoded
//! with `.insn` here (the encodings are the same as in `core`).

use core::arch::asm;

/// HFENCE.VVMA zero, zero: invalidates all guest-virtual translations.
#[inline(always)]
pub unsafe fn hfence_vvma_all() {
    asm!(".insn r 0x73, 0, 0x11, x0, x0, x0", options(nostack));
}

/// HFENCE.GVMA zero, zero: invalidates all guest-physical translations.
#[inline(always)]
pub unsafe fn hfence_gvma_all() {
    asm!(".insn r 0x73, 0, 0x31, x0, x0, x0", options(nostack));
}

/// HLVX.HU: reads a guest halfword with execute permission (guest code).
#[inline(always)]
pub unsafe fn hlvx_hu(src: *const u16) -> u16 {
    let value: usize;
    asm!(".insn r 0x73, 4, 0x32, {}, {}, x3", out(reg) value, in(reg) src, options(readonly, nostack));
    value as u16
}

/// HLV.WU: reads a guest word as if in VS/VU mode.
#[inline(always)]
pub unsafe fn hlv_wu(src: *const u32) -> u32 {
    let value: usize;
    asm!(".insn r 0x73, 4, 0x34, {}, {}, x1", out(reg) value, in(reg) src, options(readonly, nostack));
    value as u32
}
