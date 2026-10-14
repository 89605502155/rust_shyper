// Copyright (c) 2026 Andrey Ferubko.
// trosna-shyper is licensed under Mulan PSL v2.
// You can use this software according to the terms and conditions of the Mulan PSL v2.
// You may obtain a copy of Mulan PSL v2 at:
//          http://license.coscl.org.cn/MulanPSL2
// THIS SOFTWARE IS PROVIDED ON AN "AS IS" BASIS, WITHOUT WARRANTIES OF ANY KIND,
// EITHER EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO NON-INFRINGEMENT,
// MERCHANTABILITY OR FIT FOR A PARTICULAR PURPOSE.
// See the Mulan PSL v2 for more details.

//! System interfaces used by the CLI: the `/dev/shyper` device of the MVM
//! kernel module and huge page buffers shared with the hypervisor. They use
//! rustix, which issues the Linux system calls directly from Rust.

use std::{
    ffi::c_void,
    fs::File,
    io::{self, Read, Seek, SeekFrom},
    os::fd::{AsFd, BorrowedFd, OwnedFd},
    path::Path,
    ptr::{null_mut, NonNull},
};

use rustix::{
    fs::{open, Mode, OFlags},
    ioctl::{ioctl, Ioctl, IoctlOutput, Opcode},
    mm::{mmap_anonymous, munmap, MapFlags, ProtFlags},
};

pub const SHYPER_DEV: &str = "/dev/shyper";

/// An ioctl whose argument is either an integer or a pointer, as the kernel
/// module expects for the given request.
struct RawIoctl {
    request: Opcode,
    arg: *mut c_void,
}

// SAFETY: callers of `Shyper::ioctl_*` guarantee that `arg` is what the
// module expects for `request`.
unsafe impl Ioctl for RawIoctl {
    type Output = IoctlOutput;
    const IS_MUTATING: bool = true;

    fn opcode(&self) -> Opcode {
        self.request
    }

    fn as_ptr(&mut self) -> *mut c_void {
        self.arg
    }

    unsafe fn output_from_ptr(out: IoctlOutput, _: *mut c_void) -> rustix::io::Result<IoctlOutput> {
        Ok(out)
    }
}

/// The `/dev/shyper` device of the hypervisor kernel module in the MVM.
pub struct Shyper(OwnedFd);

impl Shyper {
    pub fn open() -> io::Result<Self> {
        Ok(Self(open(SHYPER_DEV, OFlags::RDWR | OFlags::CLOEXEC, Mode::empty())?))
    }

    fn ioctl_any(&self, request: usize, arg: *mut c_void) -> io::Result<IoctlOutput> {
        // SAFETY: see `RawIoctl`; the module only accesses `arg` during the call.
        Ok(unsafe {
            ioctl(
                &self.0,
                RawIoctl {
                    request: request as Opcode,
                    arg,
                },
            )?
        })
    }

    fn ioctl(&self, request: usize, arg: *mut c_void) -> io::Result<IoctlOutput> {
        let out = self.ioctl_any(request, arg)?;
        // Most requests report failures with any non-zero value.
        if out != 0 {
            return Err(io::Error::other(format!("ioctl {:#x} returned {}", request, out)));
        }
        Ok(out)
    }

    /// An ioctl that passes `arg` by value.
    pub fn ioctl_val(&self, request: usize, arg: u64) -> io::Result<()> {
        self.ioctl(request, arg as usize as *mut c_void).map(drop)
    }

    /// Like `ioctl_val`, but only negative results are errors.
    pub fn ioctl_val_any(&self, request: usize, arg: u64) -> io::Result<IoctlOutput> {
        self.ioctl_any(request, arg as usize as *mut c_void)
    }

    /// Like `ioctl_ref`, but only negative results are errors.
    ///
    /// # Safety
    /// See `ioctl_ref`.
    pub unsafe fn ioctl_ref_any<T>(&self, request: usize, arg: &mut T) -> io::Result<IoctlOutput> {
        self.ioctl_any(request, arg as *mut T as *mut c_void)
    }

    /// An ioctl that passes a pointer to `arg`, which the module may update.
    ///
    /// # Safety
    /// `T` must have the layout the module expects for `request`, including
    /// the memory any addresses stored in `arg` point to.
    pub unsafe fn ioctl_ref<T>(&self, request: usize, arg: &mut T) -> io::Result<()> {
        self.ioctl(request, arg as *mut T as *mut c_void).map(drop)
    }
}

impl AsFd for Shyper {
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.0.as_fd()
    }
}

/// An anonymous, huge page backed memory area used to exchange data with the
/// hypervisor (it is physically contiguous within a huge page).
pub struct HugeBuf {
    ptr: NonNull<u8>,
    len: usize,
}

impl HugeBuf {
    pub fn new(len: usize) -> io::Result<Self> {
        // SAFETY: a new private anonymous mapping does not alias any memory.
        let ptr = unsafe {
            mmap_anonymous(
                null_mut(),
                len,
                ProtFlags::READ | ProtFlags::WRITE,
                MapFlags::PRIVATE | MapFlags::HUGETLB,
            )?
        };
        Ok(Self {
            ptr: NonNull::new(ptr as *mut u8).ok_or_else(|| io::Error::other("mmap returned null"))?,
            len,
        })
    }

    pub fn addr(&self) -> u64 {
        self.ptr.as_ptr() as u64
    }

    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        // SAFETY: as above, and `&mut self` makes the access exclusive.
        unsafe { std::slice::from_raw_parts_mut(self.ptr.as_ptr(), self.len) }
    }

    /// Keeps the mapping for the lifetime of the process (it is shared with
    /// the hypervisor) and returns its address.
    pub fn leak(self) -> u64 {
        let addr = self.addr();
        std::mem::forget(self);
        addr
    }
}

impl Drop for HugeBuf {
    fn drop(&mut self) {
        // SAFETY: the mapping was created by `new` and is not used any more.
        let _ = unsafe { munmap(self.ptr.as_ptr() as *mut c_void, self.len) };
    }
}

/// Opens a block device or image file for unbuffered I/O.
pub fn open_direct(path: &str) -> io::Result<File> {
    Ok(File::from(open(
        path,
        OFlags::RDWR | OFlags::DIRECT | OFlags::CLOEXEC,
        Mode::empty(),
    )?))
}

/// Size in bytes of a block device or a regular file.
pub fn device_size(path: impl AsRef<Path>) -> io::Result<u64> {
    File::open(path)?.seek(SeekFrom::End(0))
}

/// Size of a memory page.
pub fn page_size() -> usize {
    rustix::param::page_size()
}

/// Translates a virtual address of this process to a physical address using
/// /proc/self/pagemap (needs CAP_SYS_ADMIN).
pub fn virt_to_phys(vaddr: u64) -> Result<u64, String> {
    const PAGEMAP: &str = "/proc/self/pagemap";
    let page_size = page_size() as u64;
    let mut file = File::open(PAGEMAP).map_err(|err| format!("Open {} err: {}", PAGEMAP, err))?;
    let offset = vaddr / page_size * 8;
    file.seek(SeekFrom::Start(offset))
        .map_err(|_| format!("File {} is not big enough to access offset {}", PAGEMAP, offset))?;
    let mut entry = [0u8; 8];
    file.read_exact(&mut entry)
        .map_err(|_| "Read page table entry err".to_string())?;
    let entry = u64::from_ne_bytes(entry);
    // Bit 63: page present, bits 0-54: page frame number.
    if entry & (1 << 63) == 0 {
        return Err(format!(
            "Virtual Address {:#x} converts to paddr err: page not in memory",
            vaddr
        ));
    }
    let pfn = entry & ((1 << 55) - 1);
    Ok(pfn * page_size + vaddr % page_size)
}
