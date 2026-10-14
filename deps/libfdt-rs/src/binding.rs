use crate::raw;
use crate::*;

use alloc::borrow::{Borrow, BorrowMut, ToOwned};
use alloc::vec;
use alloc::vec::Vec;
use core::ffi::CStr;
use core::ops::{Deref, DerefMut};

/// A device tree blob in memory; its size is the header's `totalsize`.
#[derive(Debug)]
#[repr(transparent)]
pub struct Fdt;

/// An owned device tree blob.
#[derive(Debug, Clone)]
pub struct FdtBuf {
    inner: Vec<u8>,
}

const FDT_V1_SIZE: usize = 7 * 4;

fn wrap(r: raw::FdtResult<()>) -> Result<()> {
    r.map_err(Error::Raw)
}

fn validate_bytes(data: &[u8]) -> Result<&[u8]> {
    if data.len() < FDT_V1_SIZE || raw::totalsize(data) > data.len() {
        return Err(Error::Invalid);
    }
    Ok(data)
}

impl Fdt {
    pub fn from_bytes(data: &[u8]) -> Result<&Self> {
        validate_bytes(data)?;
        Ok(unsafe { Self::from_bytes_unchecked(data) })
    }

    pub fn from_bytes_mut(data: &mut [u8]) -> Result<&mut Self> {
        validate_bytes(data)?;
        Ok(unsafe { Self::from_bytes_mut_unchecked(data) })
    }

    /// # Safety
    /// `data` must hold a device tree of at least `totalsize` bytes.
    pub unsafe fn from_bytes_unchecked(data: &[u8]) -> &Self {
        &*(data.as_ptr() as *const Self)
    }

    /// # Safety
    /// `data` must hold a device tree of at least `totalsize` bytes.
    pub unsafe fn from_bytes_mut_unchecked(data: &mut [u8]) -> &mut Self {
        &mut *(data.as_mut_ptr() as *mut Self)
    }

    /// # Safety
    /// `data` must point to a device tree of `totalsize` readable bytes.
    pub unsafe fn from_ptr<'a>(data: *const u8) -> &'a Self {
        &*(data as *const Self)
    }

    /// # Safety
    /// `data` must point to a device tree of `totalsize` writable bytes.
    pub unsafe fn from_mut_ptr<'a>(data: *mut u8) -> &'a mut Self {
        &mut *(data as *mut Self)
    }

    pub fn as_bytes(&self) -> &[u8] {
        unsafe { core::slice::from_raw_parts(self.as_ptr(), self.len()) }
    }

    pub fn as_bytes_mut(&mut self) -> &mut [u8] {
        unsafe { core::slice::from_raw_parts_mut(self.as_mut_ptr(), self.len()) }
    }

    pub fn as_ptr(&self) -> *const u8 {
        self as *const Self as *const u8
    }

    pub fn as_mut_ptr(&mut self) -> *mut u8 {
        self as *mut Self as *mut u8
    }

    pub fn len(&self) -> usize {
        // SAFETY: a `Fdt` always points to at least a device tree header.
        let header = unsafe { core::slice::from_raw_parts(self.as_ptr(), 8) };
        raw::totalsize(header)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn pack(&mut self) -> Result<()> {
        wrap(raw::pack(self.as_bytes_mut()))
    }

    pub fn overlay_apply(&mut self, overlay: &mut Fdt) -> Result<()> {
        wrap(overlay::overlay_apply(self.as_bytes_mut(), overlay.as_bytes_mut()))
    }

    pub fn remove_node(&mut self, path: &CStr) -> Result<()> {
        let b = self.as_bytes_mut();
        let node = raw::path_offset(b, path.to_bytes()).map_err(Error::Raw)?;
        wrap(raw::del_node(b, node))
    }
}

impl Deref for FdtBuf {
    type Target = Fdt;

    fn deref(&self) -> &Self::Target {
        unsafe { Fdt::from_bytes_unchecked(&self.inner) }
    }
}

impl DerefMut for FdtBuf {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { Fdt::from_bytes_mut_unchecked(&mut self.inner) }
    }
}

impl Borrow<Fdt> for FdtBuf {
    fn borrow(&self) -> &Fdt {
        self
    }
}

impl BorrowMut<Fdt> for FdtBuf {
    fn borrow_mut(&mut self) -> &mut Fdt {
        self
    }
}

impl ToOwned for Fdt {
    type Owned = FdtBuf;

    fn to_owned(&self) -> Self::Owned {
        FdtBuf::from_fdt(self)
    }
}

impl FdtBuf {
    pub fn from_fdt(fdt: &Fdt) -> Self {
        Self {
            inner: fdt.as_bytes().to_vec(),
        }
    }

    /// Copies `fdt` into a new buffer of `cap` bytes, ready to be edited.
    pub fn from_fdt_capacity(fdt: &Fdt, cap: usize) -> Result<Self> {
        let mut buf = vec![0u8; cap];
        wrap(raw::open_into(fdt.as_bytes(), &mut buf))?;
        Ok(Self { inner: buf })
    }

    pub fn reserve(&mut self, additional: usize) -> Result<()> {
        let cap = self.len() + additional;
        let new = Self::from_fdt_capacity(self, cap)?;
        *self = new;
        Ok(())
    }

    /// The buffer, truncated to the blob's `totalsize`.
    pub fn into_inner(mut self) -> Vec<u8> {
        let len = self.len();
        self.inner.truncate(len);
        self.inner
    }
}
