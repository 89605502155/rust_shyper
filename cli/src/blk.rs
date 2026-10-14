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
    os::{linux::fs::MetadataExt, unix::fs::FileExt},
    process::Command,
    sync::Mutex,
};
use log::{info, warn};
use once_cell::sync::OnceCell;

use crate::{
    hvc::{generate_hvc_mode, HVC_MEDIATED, HVC_MEDIATED_DEV_NOTIFY},
    dev::{device_size, open_direct, virt_to_phys, HugeBuf, Shyper},
    ioctl_arg::{IOCTL_SYS, IOCTL_SYS_APPEND_MED_BLK},
    util::{check_cache_address, cstr_arr_to_string, string_to_cstr_arr},
};

pub const HUGE_TLB_MAX: usize = 32 * 1024 * 1024;
pub const BLOCK_SIZE: usize = 512;

/// ioctl telling the hypervisor that a mediated request has been served.
const IOCTL_MEDIATED_FINISH: usize = generate_hvc_mode(HVC_MEDIATED, HVC_MEDIATED_DEV_NOTIFY);

pub static SHYPER: OnceCell<Shyper> = OnceCell::new();

#[derive(Debug, Clone)]
#[repr(C)]
pub struct MediatedBlkCfg {
    name: [u8; 32],
    block_dev_path: [u8; 32],
    block_num: u64,
    dma_block_max: u64,
    cache_size: u64,
    idx: u16,
    pcache: bool,
    cache_va: u64,
    cache_ipa: u64,
    cache_pa: u64,
}

impl MediatedBlkCfg {
    /// The cache shared with the hypervisor.
    fn cache(&self) -> &'static mut [u8] {
        // SAFETY: `cache_va` is a leaked `HugeBuf` of `cache_size` bytes (see
        // `mediated_blk_add`); requests for one disk are served one by one.
        unsafe { std::slice::from_raw_parts_mut(self.cache_va as *mut u8, self.cache_size as usize) }
    }
}

// Set this only once in the config_daemon
pub static MED_BLK_LIST: OnceCell<Vec<MediatedBlkCfg>> = OnceCell::new();
pub static IMG_FILES: Mutex<Vec<Option<File>>> = Mutex::new(Vec::new());

/// Clamps a request to the size of the cache.
fn clamp_count(op: &str, blk_cfg: &MediatedBlkCfg, count: u64) -> u64 {
    if count > blk_cfg.dma_block_max {
        warn!(
            "{} count {} > dma_block_max {}, shrink count to {}",
            op, count, blk_cfg.dma_block_max, blk_cfg.dma_block_max
        );
        return blk_cfg.dma_block_max;
    }
    count
}

// Read the count blocks of the blk id starting from the lba sector
// Note: do not exceed cache_size!
#[inline(always)]
fn blk_read(blk_id: u16, lba: u64, count: u64) {
    let blk_cfg = &MED_BLK_LIST.get().unwrap()[blk_id as usize];
    let files = IMG_FILES.lock().unwrap();
    let img_file = files[blk_id as usize].as_ref().unwrap();
    let len = clamp_count("blk_read", blk_cfg, count) as usize * BLOCK_SIZE;

    match img_file.read_at(&mut blk_cfg.cache()[..len], lba * BLOCK_SIZE as u64) {
        Err(err) => warn!("read lba {:#x} size {:#x} failed: {}", lba, len, err),
        Ok(n) if n != len => warn!("read lba {:#x} size {:#x} failed! read_len = {:#x}", lba, len, n),
        Ok(_) => {}
    }
}

fn blk_write(blk_id: u16, lba: u64, count: u64) {
    let blk_cfg = &MED_BLK_LIST.get().unwrap()[blk_id as usize];
    let files = IMG_FILES.lock().unwrap();
    let img_file = files[blk_id as usize].as_ref().unwrap();
    let len = clamp_count("blk_write", blk_cfg, count) as usize * BLOCK_SIZE;

    match img_file.write_at(&blk_cfg.cache()[..len], lba * BLOCK_SIZE as u64) {
        Err(err) => warn!("write lba {:#x} size {:#x} failed: {}", lba, len, err),
        Ok(n) if n != len => warn!("write lba {:#x} size {:#x} failed! write_len = {:#x}", lba, len, n),
        Ok(_) => {}
    }
}

// Read/write sector 0 of the disk to test whether the disk is ready
fn blk_try_rw(blk_id: u16) -> Result<(), String> {
    let cache = MED_BLK_LIST.get().unwrap()[blk_id as usize].cache();

    // Read origin data, and save it in origin_data
    blk_read(blk_id, 0, 1);
    let mut origin_data = [0u8; BLOCK_SIZE];
    origin_data.copy_from_slice(&cache[..BLOCK_SIZE]);

    for (i, b) in cache[..BLOCK_SIZE].iter_mut().enumerate() {
        *b = (i % 256) as u8;
    }
    blk_write(blk_id, 0, 1);

    cache[..BLOCK_SIZE].fill(0);
    blk_read(blk_id, 0, 1);

    // Check if the data written before is read correctly
    if cache[..BLOCK_SIZE]
        .iter()
        .enumerate()
        .any(|(i, &b)| b != (i % 256) as u8)
    {
        return Err(format!("blk {} read write test failed!", blk_id));
    }

    // Written back
    cache[..BLOCK_SIZE].copy_from_slice(&origin_data);
    blk_write(blk_id, 0, 1);
    Ok(())
}

pub fn mediated_blk_init() {
    // Kernel boot options cmdline:
    // default_hugepagesz=32M hugepagesz=32M hugepages=1
    // mount hugetlbfs
    // mkdir /mnt/huge
    // mount -t hugetlbfs -o pagesize=32M none /mnt/huge
    let med_blk_list = MED_BLK_LIST.get().expect("med_blk_list is None");
    if med_blk_list.is_empty() {
        warn!("NO mediated block device!");
    }
    IMG_FILES.lock().unwrap().resize_with(med_blk_list.len(), || None);

    if let Err(err) = fs::create_dir_all("/mnt/huge") {
        warn!("mkdir /mnt/huge failed: {}", err);
        return;
    }

    let output = Command::new("mount")
        .arg("-t")
        .arg("hugetlbfs")
        .arg("-o")
        .arg("pagesize=32M")
        .arg("none")
        .arg("/mnt/huge")
        .output()
        .expect("failed to execute mount");

    if !output.status.success() {
        warn!("mount hugetlbfs failed");
        return;
    }

    let shyper = match Shyper::open() {
        Ok(shyper) => SHYPER.get_or_init(|| shyper),
        Err(err) => {
            warn!("open /dev/shyper failed: {}", err);
            return;
        }
    };

    for (i, blk) in med_blk_list.iter().enumerate() {
        let name = cstr_arr_to_string(blk.name.as_slice());
        let path = cstr_arr_to_string(blk.block_dev_path.as_slice());
        info!("Shyper daemon init blk {} with cache size {}", name, blk.cache_size);
        info!(
            "Shyper daemon init blk {} va {:#x} with cache pa {:#x}",
            name, blk.cache_va, blk.cache_ipa
        );

        match open_direct(&path) {
            Ok(file) => IMG_FILES.lock().unwrap()[i] = Some(file),
            Err(err) => {
                warn!("open block device {} failed: {}", path, err);
                return;
            }
        }

        if let Err(err) = blk_try_rw(i as u16) {
            warn!("blk_try_rw failed: {}", err);
            return;
        }

        let request = generate_hvc_mode(IOCTL_SYS, IOCTL_SYS_APPEND_MED_BLK);
        let mut cfg = blk.clone();
        // SAFETY: the module reads a `MediatedBlkCfg`.
        if let Err(err) = unsafe { shyper.ioctl_ref(request, &mut cfg) } {
            warn!("ioctl append mediated blk failed: {}", err);
            return;
        }

        info!("Shyper daemon init blk {} success", name);
    }
}

fn mediated_blk_finish(blk_id: u16) {
    if let Err(err) = SHYPER.get().unwrap().ioctl_val(IOCTL_MEDIATED_FINISH, blk_id as u64) {
        warn!("Mediated blk finish ioctl failed: {}", err);
    }
}

// mediated_blk_read: Do reading, and after that send finishing signal to kernel module
pub fn mediated_blk_read(blk_id: u16, lba: u64, count: u64) {
    blk_read(blk_id, lba, count);
    mediated_blk_finish(blk_id);
}

pub fn mediated_blk_write(blk_id: u16, lba: u64, count: u64) {
    blk_write(blk_id, lba, count);
    mediated_blk_finish(blk_id);
}

// Add a mediated blk
pub fn mediated_blk_add(index: usize, dev: String) -> Result<MediatedBlkCfg, String> {
    const S_IFMT: u32 = 0o170000;
    const S_IFBLK: u32 = 0o060000;
    let metadata = fs::metadata(&dev).map_err(|x| format!("metadata err: {}", x))?;
    if metadata.st_mode() & S_IFMT != S_IFBLK {
        warn!(
            "{} is not a block device, but we can also use {} as a img file",
            dev, dev
        );
    }

    let nsec = device_size(&dev).map_err(|err| format!("get size of {} err: {}", dev, err))? / BLOCK_SIZE as u64;
    info!("Shyper daemon add blk {} with {} sectors", dev, nsec);

    let cache_size = HUGE_TLB_MAX;
    let mut cache = HugeBuf::new(cache_size).map_err(|err| {
        warn!("mmap cache failed: {}", err);
        "mmap cache failed".to_string()
    })?;

    if let Err(err) = check_cache_address(cache.as_mut_slice()) {
        warn!("check cache address failed: {}", err);
        return Err("check cache address failed".to_string());
    }

    let cache_ipa = virt_to_phys(cache.addr()).map_err(|err| {
        warn!("virt_to_phys failed: {}", err);
        format!("virt_to_phys failed: {}", err)
    })?;

    Ok(MediatedBlkCfg {
        name: string_to_cstr_arr(format!("MEDBLK{}", index)),
        block_dev_path: string_to_cstr_arr(dev.clone()),
        block_num: nsec,
        dma_block_max: (cache_size / BLOCK_SIZE) as u64,
        cache_size: cache_size as u64,
        idx: index as u16,
        pcache: false,
        // The cache stays mapped: it is shared with the hypervisor.
        cache_va: cache.leak(),
        cache_ipa,
        cache_pa: 0,
    })
}
