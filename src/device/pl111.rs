// Copyright (c) 2026 Andrey Ferubko.
// trosna-shyper is licensed under Mulan PSL v2.
// You can use this software according to the terms and conditions of the Mulan PSL v2.
// You may obtain a copy of Mulan PSL v2 at:
//          http://license.coscl.org.cn/MulanPSL2
// THIS SOFTWARE IS PROVIDED ON AN "AS IS" BASIS, WITHOUT WARRANTIES OF ANY KIND,
// EITHER EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO NON-INFRINGEMENT,
// MERCHANTABILITY OR FIT FOR A PARTICULAR PURPOSE.
// See the Mulan PSL v2 for more details.

//! Emulated ARM PrimeCell PL111 colour LCD controller for a VM on QEMU.
//!
//! The VM programs it like real hardware (the Linux `amba-clcd` driver);
//! when it enables the panel, the frame buffer is handed to QEMU's ramfb
//! display, which shows it in the QEMU window. Only VMs whose memory is
//! mapped 1:1 (the MVM) can use it: ramfb reads guest physical addresses.

use core::ops::Range;

use spin::Mutex;

use crate::config::VmEmulatedDeviceConfig;
use crate::device::{EmuContext, EmuDev, EmuDeviceType};
use crate::driver::{fw_cfg_find_file, fw_cfg_write};
use crate::kernel::current_cpu;

const TIM0: usize = 0x000;
const TIM1: usize = 0x004;
const TIM2: usize = 0x008;
const TIM3: usize = 0x00c;
const UPBASE: usize = 0x010;
const LPBASE: usize = 0x014;
const CNTL: usize = 0x018;
const IMSC: usize = 0x01c;
const RIS: usize = 0x020;
const MIS: usize = 0x024;
const ICR: usize = 0x028;
const UPCURR: usize = 0x02c;
const LPCURR: usize = 0x030;
const PERIPH_ID: usize = 0xfe0;

/// PL111 identification: part 0x111, designer ARM, revision 2; PrimeCell id.
const ID: [u32; 8] = [0x11, 0x11, 0x24, 0x00, 0x0d, 0xf0, 0x05, 0xb1];

const CNTL_EN: u32 = 1 << 0;
const CNTL_BPP_SHIFT: u32 = 1;
const CNTL_BGR: u32 = 1 << 8;
const CNTL_PWR: u32 = 1 << 11;
/// RIS bit set while the base address can be updated.
const INT_BASE_UPDATE: u32 = 1 << 2;

const fn fourcc(s: &[u8; 4]) -> u32 {
    u32::from_le_bytes(*s)
}
const DRM_FORMAT_XRGB8888: u32 = fourcc(b"XR24");
const DRM_FORMAT_XBGR8888: u32 = fourcc(b"XB24");
const DRM_FORMAT_RGB565: u32 = fourcc(b"RG16");
const DRM_FORMAT_BGR565: u32 = fourcc(b"BG16");

#[derive(Default)]
struct Regs {
    tim: [u32; 4],
    upbase: u32,
    lpbase: u32,
    cntl: u32,
    imsc: u32,
    /// The mode last handed to ramfb, to avoid reprogramming it.
    shown: Option<(u32, u32, u32, u32)>,
}

impl Regs {
    /// Hands the frame buffer to ramfb when the panel is on.
    fn update_display(&mut self) {
        if self.cntl & (CNTL_EN | CNTL_PWR) != (CNTL_EN | CNTL_PWR) || self.upbase == 0 {
            return;
        }
        let width = (((self.tim[0] >> 2) & 0x3f) + 1) * 16;
        let height = (self.tim[1] & 0x3ff) + 1;
        let bgr = self.cntl & CNTL_BGR != 0;
        let (format, bytes) = match (self.cntl >> CNTL_BPP_SHIFT) & 0x7 {
            // 24 bpp: one 32-bit word per pixel, red in the low byte.
            5 => (if bgr { DRM_FORMAT_XRGB8888 } else { DRM_FORMAT_XBGR8888 }, 4),
            4 | 6 => (if bgr { DRM_FORMAT_RGB565 } else { DRM_FORMAT_BGR565 }, 2),
            bpp => {
                warn!("pl111: unsupported pixel depth {}", bpp);
                return;
            }
        };
        let mode = (self.upbase, width, height, format);
        if self.shown == Some(mode) {
            return;
        }
        match ramfb_show(self.upbase as u64, format, width, height, width * bytes) {
            Ok(()) => {
                info!(
                    "pl111: {}x{} frame buffer at {:#x} shown by ramfb",
                    width, height, self.upbase
                );
                self.shown = Some(mode);
            }
            Err(()) => warn!("pl111: QEMU ramfb is not available (start QEMU with -device ramfb)"),
        }
    }

    fn read(&self, off: usize) -> u32 {
        match off {
            TIM0..=TIM3 => self.tim[(off - TIM0) / 4],
            UPBASE | UPCURR => self.upbase,
            LPBASE | LPCURR => self.lpbase,
            CNTL => self.cntl,
            IMSC => self.imsc,
            RIS => INT_BASE_UPDATE,
            MIS => INT_BASE_UPDATE & self.imsc,
            PERIPH_ID..=0xffc => ID[(off - PERIPH_ID) / 4],
            _ => 0,
        }
    }

    fn write(&mut self, off: usize, val: u32) {
        match off {
            TIM0..=TIM3 => self.tim[(off - TIM0) / 4] = val,
            UPBASE => self.upbase = val,
            LPBASE => self.lpbase = val,
            CNTL => self.cntl = val,
            IMSC => self.imsc = val,
            // ICR, palette and cursor: nothing to emulate.
            _ => return,
        }
        if matches!(off, TIM0 | TIM1 | UPBASE | CNTL) {
            self.update_display();
        }
    }
}

/// Configures QEMU's ramfb to scan out `addr`.
fn ramfb_show(addr: u64, format: u32, width: u32, height: u32, stride: u32) -> Result<(), ()> {
    static RAMFB: spin::Once<Option<u16>> = spin::Once::new();
    let selector = RAMFB.call_once(|| fw_cfg_find_file("etc/ramfb")).ok_or(())?;
    // struct RAMFBCfg, big-endian: addr, fourcc, flags, width, height, stride.
    let mut cfg = [0u8; 28];
    cfg[0..8].copy_from_slice(&addr.to_be_bytes());
    cfg[8..12].copy_from_slice(&format.to_be_bytes());
    cfg[16..20].copy_from_slice(&width.to_be_bytes());
    cfg[20..24].copy_from_slice(&height.to_be_bytes());
    cfg[24..28].copy_from_slice(&stride.to_be_bytes());
    fw_cfg_write(selector, &cfg)
}

pub struct EmuPl111 {
    base_ipa: usize,
    length: usize,
    regs: Mutex<Regs>,
}

impl EmuDev for EmuPl111 {
    fn emu_type(&self) -> EmuDeviceType {
        EmuDeviceType::EmuDeviceTPl111
    }

    fn address_range(&self) -> Range<usize> {
        self.base_ipa..self.base_ipa + self.length
    }

    fn handler(&self, emu_ctx: &EmuContext) -> bool {
        let off = emu_ctx.address - self.base_ipa;
        let mut regs = self.regs.lock();
        if emu_ctx.write {
            let val = current_cpu().get_gpr(emu_ctx.reg) as u32;
            regs.write(off, val);
        } else {
            current_cpu().set_gpr(emu_ctx.reg, regs.read(off) as usize);
        }
        true
    }
}

/// Creates the emulated PL111 of `emu_cfg`.
pub fn emu_pl111_init(emu_cfg: &VmEmulatedDeviceConfig) -> Result<alloc::sync::Arc<dyn EmuDev>, ()> {
    Ok(alloc::sync::Arc::new(EmuPl111 {
        base_ipa: emu_cfg.base_ipa,
        length: emu_cfg.length,
        regs: Mutex::new(Regs::default()),
    }))
}
