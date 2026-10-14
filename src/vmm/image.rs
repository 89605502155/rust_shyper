// Copyright (c) 2026 Andrey Ferubko.
// trosna-shyper is licensed under Mulan PSL v2.
// You can use this software according to the terms and conditions of the Mulan PSL v2.
// You may obtain a copy of Mulan PSL v2 at:
//          http://license.coscl.org.cn/MulanPSL2
// THIS SOFTWARE IS PROVIDED ON AN "AS IS" BASIS, WITHOUT WARRANTIES OF ANY KIND,
// EITHER EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO NON-INFRINGEMENT,
// MERCHANTABILITY OR FIT FOR A PARTICULAR PURPOSE.
// See the Mulan PSL v2 for more details.

//! The MVM kernel image embedded into the hypervisor binary.
//!
//! The image path comes from `VM0_IMAGE_PATH` at build time (see build.rs).
//! The image is placed in the `.vm0image` section, see linkers/*.ld.

core::arch::global_asm!(
    ".pushsection .vm0image, \"a\"",
    ".balign 16",
    ".global _binary_vm0img_start",
    "_binary_vm0img_start:",
    concat!(".incbin \"", env!("SHYPER_VM0_IMAGE"), "\""),
    ".global _binary_vm0img_end",
    "_binary_vm0img_end:",
    ".popsection",
);

extern "C" {
    static _binary_vm0img_start: u8;
    static _binary_vm0img_end: u8;
}

/// The embedded MVM kernel image.
pub fn vm0_image() -> &'static [u8] {
    // SAFETY: both symbols are defined above around the embedded image, which
    // is read-only data for the whole lifetime of the hypervisor.
    unsafe {
        let start = core::ptr::addr_of!(_binary_vm0img_start);
        let end = core::ptr::addr_of!(_binary_vm0img_end);
        core::slice::from_raw_parts(start, end as usize - start as usize)
    }
}
