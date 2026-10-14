// Copyright (c) 2026 Andrey Ferubko.
// trosna-shyper is licensed under Mulan PSL v2.
// You can use this software according to the terms and conditions of the Mulan PSL v2.
// You may obtain a copy of Mulan PSL v2 at:
//          http://license.coscl.org.cn/MulanPSL2
// THIS SOFTWARE IS PROVIDED ON AN "AS IS" BASIS, WITHOUT WARRANTIES OF ANY KIND,
// EITHER EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO NON-INFRINGEMENT,
// MERCHANTABILITY OR FIT FOR A PARTICULAR PURPOSE.
// See the Mulan PSL v2 for more details.

//! Feature `hang-debug`: periodic dump of the interrupt state of core 0 (the
//! MVM core), to find out why a guest stops getting interrupts.

use core::sync::atomic::{AtomicUsize, Ordering};

use cortex_a::registers::{Readable, CNTVCT_EL0, CNTV_CTL_EL0, CNTV_CVAL_EL0};

use super::{gic_lrs, GICD, GICH};
use crate::kernel::current_cpu;

/// Interrupts taken at EL2 on core 0, by id (only ids < 32 are counted).
static PPI_COUNT: [AtomicUsize; 32] = [const { AtomicUsize::new(0) }; 32];
static TICKS: AtomicUsize = AtomicUsize::new(0);

pub fn count_irq(id: usize) {
    if current_cpu().id == 0 && id < 32 {
        PPI_COUNT[id].fetch_add(1, Ordering::Relaxed);
    }
}

/// Called from the hypervisor timer handler (every 10 ms).
pub fn tick() {
    if current_cpu().id != 0 {
        return;
    }
    let t = TICKS.fetch_add(1, Ordering::Relaxed);
    if t % 50 != 0 {
        return;
    }
    let mut lrs = [0u32; 4];
    for (i, lr) in lrs.iter_mut().enumerate().take(gic_lrs().min(4)) {
        *lr = GICH.lr(i);
    }
    println!(
        "[hang-debug] t={} ppi26={} ppi27={} | GICD en={:#010x} pend={:#010x} act={:#010x} | GICH hcr={:#x} \
         misr={:#x} elrsr={:#x} lr={:08x?} | CNTV ctl={:#x} cval-cnt={}",
        t,
        PPI_COUNT[26].load(Ordering::Relaxed),
        PPI_COUNT[27].load(Ordering::Relaxed),
        GICD.is_enabler(0),
        GICD.is_pender(0),
        GICD.is_activer(0),
        GICH.hcr(),
        GICH.misr(),
        GICH.elrsr(0),
        lrs,
        CNTV_CTL_EL0.get(),
        CNTV_CVAL_EL0.get() as i64 - CNTVCT_EL0.get() as i64,
    );
}
