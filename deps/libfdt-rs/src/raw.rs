// Copyright (c) 2023 Beihang University, Huawei Technologies Co.,Ltd. All rights reserved.
// Rust-Shyper is licensed under Mulan PSL v2.
// You can use this software according to the terms and conditions of the Mulan PSL v2.
// You may obtain a copy of Mulan PSL v2 at:
//          http://license.coscl.org.cn/MulanPSL2
// THIS SOFTWARE IS PROVIDED ON AN "AS IS" BASIS, WITHOUT WARRANTIES OF ANY KIND,
// EITHER EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO NON-INFRINGEMENT,
// MERCHANTABILITY OR FIT FOR A PARTICULAR PURPOSE.
// See the Mulan PSL v2 for more details.

//! Pure Rust port of the part of libfdt (dtc/libfdt: fdt.c, fdt_ro.c,
//! fdt_rw.c, fdt_wip.c) used by Rust-Shyper.
//!
//! All functions work on a flattened device tree blob held in a byte slice
//! whose length is the capacity of the blob (normally its `totalsize`).
//! Offsets are structure block offsets, exactly as in libfdt, and errors are
//! the negative libfdt error codes, so results can be passed on unchanged.

use alloc::vec::Vec;

pub const FDT_MAGIC: u32 = 0xd00d_feed;

pub const FDT_ERR_NOTFOUND: i32 = 1;
pub const FDT_ERR_EXISTS: i32 = 2;
pub const FDT_ERR_NOSPACE: i32 = 3;
pub const FDT_ERR_BADOFFSET: i32 = 4;
pub const FDT_ERR_BADPATH: i32 = 5;
pub const FDT_ERR_BADPHANDLE: i32 = 6;
pub const FDT_ERR_BADSTATE: i32 = 7;
pub const FDT_ERR_TRUNCATED: i32 = 8;
pub const FDT_ERR_BADMAGIC: i32 = 9;
pub const FDT_ERR_BADVERSION: i32 = 10;
pub const FDT_ERR_BADSTRUCTURE: i32 = 11;
pub const FDT_ERR_BADLAYOUT: i32 = 12;
pub const FDT_ERR_INTERNAL: i32 = 13;
pub const FDT_ERR_BADNCELLS: i32 = 14;
pub const FDT_ERR_BADVALUE: i32 = 15;
pub const FDT_ERR_BADOVERLAY: i32 = 16;
pub const FDT_ERR_NOPHANDLES: i32 = 17;

/// Result of a libfdt operation, the error is a negative `FDT_ERR_*` code.
pub type FdtResult<T> = Result<T, i32>;

const fn err<T>(code: i32) -> FdtResult<T> {
    Err(-code)
}

const FDT_BEGIN_NODE: u32 = 0x1;
const FDT_END_NODE: u32 = 0x2;
const FDT_PROP: u32 = 0x3;
const FDT_NOP: u32 = 0x4;
const FDT_END: u32 = 0x9;

const TAG_SIZE: usize = 4;
const HEADER_SIZE: usize = 40;
const PROP_HEADER_SIZE: usize = 12;
const RSV_ENTRY_SIZE: usize = 16;

const LAST_SUPPORTED_VERSION: u32 = 17;
const FIRST_SUPPORTED_VERSION: u32 = 17;

const H_MAGIC: usize = 0;
const H_TOTALSIZE: usize = 4;
const H_OFF_DT_STRUCT: usize = 8;
const H_OFF_DT_STRINGS: usize = 12;
const H_OFF_MEM_RSVMAP: usize = 16;
const H_VERSION: usize = 20;
const H_LAST_COMP_VERSION: usize = 24;
const H_SIZE_DT_STRINGS: usize = 32;
const H_SIZE_DT_STRUCT: usize = 36;

const fn tag_align(x: usize) -> usize {
    (x + TAG_SIZE - 1) & !(TAG_SIZE - 1)
}

pub fn be32(b: &[u8], off: usize) -> u32 {
    u32::from_be_bytes([b[off], b[off + 1], b[off + 2], b[off + 3]])
}

pub fn set_be32(b: &mut [u8], off: usize, v: u32) {
    b[off..off + 4].copy_from_slice(&v.to_be_bytes());
}

fn be64(b: &[u8], off: usize) -> u64 {
    let mut v = [0u8; 8];
    v.copy_from_slice(&b[off..off + 8]);
    u64::from_be_bytes(v)
}

fn hdr(b: &[u8], field: usize) -> usize {
    be32(b, field) as usize
}

fn set_hdr(b: &mut [u8], field: usize, v: usize) {
    set_be32(b, field, v as u32)
}

pub fn totalsize(b: &[u8]) -> usize {
    hdr(b, H_TOTALSIZE)
}

pub fn set_magic(b: &mut [u8], magic: u32) {
    set_be32(b, H_MAGIC, magic)
}

fn off_struct(b: &[u8]) -> usize {
    hdr(b, H_OFF_DT_STRUCT)
}

fn off_strings(b: &[u8]) -> usize {
    hdr(b, H_OFF_DT_STRINGS)
}

fn off_rsvmap(b: &[u8]) -> usize {
    hdr(b, H_OFF_MEM_RSVMAP)
}

fn size_struct(b: &[u8]) -> usize {
    hdr(b, H_SIZE_DT_STRUCT)
}

fn size_strings(b: &[u8]) -> usize {
    hdr(b, H_SIZE_DT_STRINGS)
}

/// Equivalent of `fdt_check_header()` (also `FDT_RO_PROBE`).
pub fn check_header(b: &[u8]) -> FdtResult<()> {
    if b.len() < HEADER_SIZE {
        return err(FDT_ERR_TRUNCATED);
    }
    if be32(b, H_MAGIC) != FDT_MAGIC {
        return err(FDT_ERR_BADMAGIC);
    }
    let version = be32(b, H_VERSION);
    if version < FIRST_SUPPORTED_VERSION || be32(b, H_LAST_COMP_VERSION) > LAST_SUPPORTED_VERSION {
        return err(FDT_ERR_BADVERSION);
    }
    let total = totalsize(b);
    if total < HEADER_SIZE || total > b.len() {
        return err(FDT_ERR_TRUNCATED);
    }
    let in_blob = |off: usize, size: usize| off.checked_add(size).is_some_and(|end| end <= total);
    if !in_blob(off_rsvmap(b), 0)
        || !in_blob(off_struct(b), size_struct(b))
        || !in_blob(off_strings(b), size_strings(b))
    {
        return err(FDT_ERR_TRUNCATED);
    }
    Ok(())
}

/// Offset of `len` bytes at structure offset `off` inside the blob.
fn struct_ptr(b: &[u8], off: usize, len: usize) -> Option<usize> {
    let end = off.checked_add(len)?;
    if end > size_struct(b) {
        return None;
    }
    let abs = off_struct(b) + off;
    if abs + len > b.len() {
        return None;
    }
    Some(abs)
}

/// Equivalent of `fdt_next_tag()`: returns the tag at `start` and the offset
/// of the next tag.
fn next_tag(b: &[u8], start: usize) -> FdtResult<(u32, usize)> {
    let tagp = struct_ptr(b, start, TAG_SIZE).ok_or(-FDT_ERR_TRUNCATED)?;
    let tag = be32(b, tagp);
    let mut offset = start + TAG_SIZE;
    match tag {
        FDT_BEGIN_NODE => loop {
            let p = struct_ptr(b, offset, 1).ok_or(-FDT_ERR_TRUNCATED)?;
            offset += 1;
            if b[p] == 0 {
                break;
            }
        },
        FDT_PROP => {
            let lenp = struct_ptr(b, offset, 4).ok_or(-FDT_ERR_TRUNCATED)?;
            offset += PROP_HEADER_SIZE - TAG_SIZE + be32(b, lenp) as usize;
        }
        FDT_END | FDT_END_NODE | FDT_NOP => {}
        _ => return err(FDT_ERR_BADSTRUCTURE),
    }
    if struct_ptr(b, start, offset - start).is_none() {
        return err(FDT_ERR_TRUNCATED);
    }
    Ok((tag, tag_align(offset)))
}

fn check_tag_offset(b: &[u8], offset: i32, want: u32) -> FdtResult<usize> {
    if offset < 0 || !(offset as usize).is_multiple_of(TAG_SIZE) {
        return err(FDT_ERR_BADOFFSET);
    }
    match next_tag(b, offset as usize) {
        Ok((tag, next)) if tag == want => Ok(next),
        _ => err(FDT_ERR_BADOFFSET),
    }
}

fn check_node_offset(b: &[u8], offset: i32) -> FdtResult<usize> {
    check_tag_offset(b, offset, FDT_BEGIN_NODE)
}

fn check_prop_offset(b: &[u8], offset: i32) -> FdtResult<usize> {
    check_tag_offset(b, offset, FDT_PROP)
}

/// Equivalent of `fdt_next_node()`. A negative `offset` starts at the root.
///
/// As in libfdt, when the end of the node at depth 0 is reached the offset
/// right after that node is returned with `*depth < 0`.
pub fn next_node(b: &[u8], offset: i32, mut depth: Option<&mut i32>) -> FdtResult<i32> {
    let mut nextoffset = 0usize;
    if offset >= 0 {
        nextoffset = check_node_offset(b, offset)?;
    }
    loop {
        let offset = nextoffset;
        let (tag, next) = match next_tag(b, offset) {
            Ok(t) => t,
            Err(e) if e == -FDT_ERR_TRUNCATED && depth.is_none() => return err(FDT_ERR_NOTFOUND),
            Err(e) => return Err(e),
        };
        nextoffset = next;
        match tag {
            FDT_PROP | FDT_NOP => {}
            FDT_BEGIN_NODE => {
                if let Some(d) = depth.as_deref_mut() {
                    *d += 1;
                }
                return Ok(offset as i32);
            }
            FDT_END_NODE => {
                if let Some(d) = depth.as_deref_mut() {
                    *d -= 1;
                    if *d < 0 {
                        return Ok(nextoffset as i32);
                    }
                }
            }
            _ => return err(FDT_ERR_NOTFOUND),
        }
    }
}

/// Equivalent of `fdt_first_subnode()`.
pub fn first_subnode(b: &[u8], offset: i32) -> FdtResult<i32> {
    let mut depth = 0;
    match next_node(b, offset, Some(&mut depth)) {
        Ok(o) if depth == 1 => Ok(o),
        _ => err(FDT_ERR_NOTFOUND),
    }
}

/// Equivalent of `fdt_next_subnode()`.
pub fn next_subnode(b: &[u8], mut offset: i32) -> FdtResult<i32> {
    let mut depth = 1;
    loop {
        offset = match next_node(b, offset, Some(&mut depth)) {
            Ok(o) if depth >= 1 => o,
            _ => return err(FDT_ERR_NOTFOUND),
        };
        if depth == 1 {
            return Ok(offset);
        }
    }
}

/// Collects the offsets of all direct subnodes of `parent`.
///
/// Offsets are only valid as long as the blob is not modified.
pub fn subnodes(b: &[u8], parent: i32) -> Vec<i32> {
    let mut out = Vec::new();
    let mut node = first_subnode(b, parent);
    while let Ok(n) = node {
        out.push(n);
        node = next_subnode(b, n);
    }
    out
}

/// Equivalent of `fdt_get_name()`, the returned name has no trailing NUL.
pub fn get_name(b: &[u8], node: i32) -> FdtResult<&[u8]> {
    check_node_offset(b, node)?;
    let start = off_struct(b) + node as usize + TAG_SIZE;
    let len = b[start..].iter().position(|&c| c == 0).ok_or(-FDT_ERR_TRUNCATED)?;
    Ok(&b[start..start + len])
}

/// A string from the strings block, without the trailing NUL.
fn string(b: &[u8], stroffset: usize) -> FdtResult<&[u8]> {
    if stroffset >= size_strings(b) {
        return err(FDT_ERR_BADOFFSET);
    }
    let start = off_strings(b) + stroffset;
    let end = off_strings(b) + size_strings(b);
    let len = b[start..end].iter().position(|&c| c == 0).ok_or(-FDT_ERR_TRUNCATED)?;
    Ok(&b[start..start + len])
}

fn nextprop(b: &[u8], mut offset: usize) -> FdtResult<i32> {
    loop {
        let (tag, next) = next_tag(b, offset)?;
        match tag {
            FDT_END => return err(FDT_ERR_BADSTRUCTURE),
            FDT_PROP => return Ok(offset as i32),
            FDT_NOP => offset = next,
            _ => return err(FDT_ERR_NOTFOUND),
        }
    }
}

/// Equivalent of `fdt_first_property_offset()`.
pub fn first_property_offset(b: &[u8], node: i32) -> FdtResult<i32> {
    nextprop(b, check_node_offset(b, node)?)
}

/// Equivalent of `fdt_next_property_offset()`.
pub fn next_property_offset(b: &[u8], offset: i32) -> FdtResult<i32> {
    nextprop(b, check_prop_offset(b, offset)?)
}

/// Collects the offsets of all properties of `node`.
pub fn properties(b: &[u8], node: i32) -> Vec<i32> {
    let mut out = Vec::new();
    let mut prop = first_property_offset(b, node);
    while let Ok(p) = prop {
        out.push(p);
        prop = next_property_offset(b, p);
    }
    out
}

/// Property at `offset`: (name, absolute offset of the value, value length).
pub fn get_property_by_offset(b: &[u8], offset: i32) -> FdtResult<(&[u8], usize, usize)> {
    check_prop_offset(b, offset)?;
    let abs = off_struct(b) + offset as usize;
    let len = be32(b, abs + 4) as usize;
    let name = string(b, be32(b, abs + 8) as usize)?;
    Ok((name, abs + PROP_HEADER_SIZE, len))
}

/// Property `name` of `node`: (property offset, absolute value offset, length).
fn get_property(b: &[u8], node: i32, name: &[u8]) -> FdtResult<(i32, usize, usize)> {
    let mut prop = first_property_offset(b, node);
    while let Ok(p) = prop {
        let (pname, data, len) = get_property_by_offset(b, p)?;
        if pname == name {
            return Ok((p, data, len));
        }
        prop = next_property_offset(b, p);
    }
    Err(prop.unwrap_err())
}

/// Equivalent of `fdt_getprop()`.
pub fn getprop<'a>(b: &'a [u8], node: i32, name: &[u8]) -> FdtResult<&'a [u8]> {
    let (_, data, len) = get_property(b, node, name)?;
    Ok(&b[data..data + len])
}

fn nodename_eq(b: &[u8], node: i32, s: &[u8]) -> bool {
    let Ok(p) = get_name(b, node) else {
        return false;
    };
    if p.len() < s.len() || &p[..s.len()] != s {
        false
    } else if p.len() == s.len() {
        true
    } else if s.contains(&b'@') {
        false
    } else {
        p[s.len()] == b'@'
    }
}

/// Equivalent of `fdt_subnode_offset_namelen()`.
pub fn subnode_offset(b: &[u8], parent: i32, name: &[u8]) -> FdtResult<i32> {
    check_header(b)?;
    let mut depth = 0;
    let mut offset = Ok(parent);
    while let Ok(o) = offset {
        if depth < 0 {
            break;
        }
        if depth == 1 && nodename_eq(b, o, name) {
            return Ok(o);
        }
        offset = next_node(b, o, Some(&mut depth));
    }
    if depth < 0 {
        return err(FDT_ERR_NOTFOUND);
    }
    offset
}

/// Equivalent of `fdt_path_offset_namelen()`, aliases are supported.
pub fn path_offset(b: &[u8], path: &[u8]) -> FdtResult<i32> {
    check_header(b)?;
    let mut p = 0;
    let mut offset = 0;
    if path.first() != Some(&b'/') {
        let q = path.iter().position(|&c| c == b'/').unwrap_or(path.len());
        let aliases = path_offset(b, b"/aliases").map_err(|_| -FDT_ERR_BADPATH)?;
        let alias = getprop(b, aliases, &path[..q]).map_err(|_| -FDT_ERR_BADPATH)?;
        let alias = alias.split(|&c| c == 0).next().unwrap_or(&[]);
        offset = path_offset(b, alias)?;
        p = q;
    }
    while p < path.len() {
        while path[p] == b'/' {
            p += 1;
            if p == path.len() {
                return Ok(offset);
            }
        }
        let q = path[p..].iter().position(|&c| c == b'/').map_or(path.len(), |i| p + i);
        offset = subnode_offset(b, offset, &path[p..q])?;
        p = q;
    }
    Ok(offset)
}

/// Full path of `node` (equivalent of `fdt_get_path()`).
pub fn get_path(b: &[u8], node: i32) -> FdtResult<Vec<u8>> {
    check_header(b)?;
    let mut stack: Vec<i32> = Vec::new();
    let mut depth = 0;
    let mut offset = 0;
    loop {
        stack.truncate(depth as usize);
        stack.push(offset);
        if offset == node {
            break;
        }
        // `depth` becomes the depth of the returned node, the root being 0.
        offset = next_node(b, offset, Some(&mut depth))?;
        if depth < 0 {
            return err(FDT_ERR_BADOFFSET);
        }
    }
    let mut path = Vec::new();
    for &n in &stack[1..] {
        path.push(b'/');
        path.extend_from_slice(get_name(b, n)?);
    }
    if path.is_empty() {
        path.push(b'/');
    }
    Ok(path)
}

/// Equivalent of `fdt_get_phandle()`, 0 if the node has no phandle.
pub fn get_phandle(b: &[u8], node: i32) -> u32 {
    for name in [&b"phandle"[..], b"linux,phandle"] {
        if let Ok(v) = getprop(b, node, name) {
            return if v.len() == 4 { be32(v, 0) } else { 0 };
        }
    }
    0
}

/// Iterates over all nodes, starting after `start` (negative: from the root).
fn find_node(b: &[u8], start: i32, mut pred: impl FnMut(i32) -> bool) -> FdtResult<i32> {
    let mut offset = next_node(b, start, None);
    while let Ok(o) = offset {
        if pred(o) {
            return Ok(o);
        }
        offset = next_node(b, o, None);
    }
    offset
}

/// Equivalent of `fdt_node_offset_by_phandle()`.
pub fn node_offset_by_phandle(b: &[u8], phandle: u32) -> FdtResult<i32> {
    if phandle == 0 || phandle == u32::MAX {
        return err(FDT_ERR_BADPHANDLE);
    }
    check_header(b)?;
    find_node(b, -1, |o| get_phandle(b, o) == phandle)
}

/// Equivalent of `fdt_find_max_phandle()`.
pub fn find_max_phandle(b: &[u8]) -> FdtResult<u32> {
    check_header(b)?;
    let mut max = 0;
    let mut bad = false;
    let res = find_node(b, -1, |o| {
        let phandle = get_phandle(b, o);
        bad = phandle == u32::MAX;
        max = max.max(phandle);
        bad
    });
    if bad {
        return err(FDT_ERR_NOPHANDLES);
    }
    match res {
        Err(e) if e == -FDT_ERR_NOTFOUND => Ok(max),
        Err(e) => Err(e),
        Ok(_) => Ok(max),
    }
}

/// Equivalent of `fdt_node_offset_by_prop_value()`.
pub fn node_offset_by_prop_value(b: &[u8], start: i32, name: &[u8], val: &[u8]) -> FdtResult<i32> {
    check_header(b)?;
    find_node(b, start, |o| getprop(b, o, name).is_ok_and(|v| v == val))
}

fn stringlist_contains(list: &[u8], s: &[u8]) -> bool {
    list.split(|&c| c == 0).any(|item| item == s)
}

/// Equivalent of `fdt_node_offset_by_compatible()`.
pub fn node_offset_by_compatible(b: &[u8], start: i32, compatible: &[u8]) -> FdtResult<i32> {
    check_header(b)?;
    find_node(b, start, |o| {
        getprop(b, o, b"compatible").is_ok_and(|v| stringlist_contains(v, compatible))
    })
}

/// Number of memory reservation entries (equivalent of `fdt_num_mem_rsv()`).
pub fn num_mem_rsv(b: &[u8]) -> FdtResult<usize> {
    let base = off_rsvmap(b);
    let mut i = 0;
    loop {
        let off = base + i * RSV_ENTRY_SIZE;
        if off + RSV_ENTRY_SIZE > totalsize(b) {
            return err(FDT_ERR_TRUNCATED);
        }
        if be64(b, off) == 0 && be64(b, off + 8) == 0 {
            return Ok(i);
        }
        i += 1;
    }
}

// ---------------------------------------------------------------------------
// Read-write functions (fdt_rw.c, fdt_wip.c)
// ---------------------------------------------------------------------------

fn blocks_misordered(b: &[u8], mem_rsv_size: usize, struct_size: usize) -> bool {
    off_rsvmap(b) < HEADER_SIZE.next_multiple_of(8)
        || off_struct(b) < off_rsvmap(b) + mem_rsv_size
        || off_strings(b) < off_struct(b) + struct_size
        || totalsize(b) < off_strings(b) + size_strings(b)
}

fn rw_probe(b: &mut [u8]) -> FdtResult<()> {
    check_header(b)?;
    if be32(b, H_VERSION) < 17 {
        return err(FDT_ERR_BADVERSION);
    }
    let rsv = (num_mem_rsv(b)? + 1) * RSV_ENTRY_SIZE;
    if blocks_misordered(b, rsv, size_struct(b)) {
        return err(FDT_ERR_BADLAYOUT);
    }
    if be32(b, H_VERSION) > 17 {
        set_be32(b, H_VERSION, 17);
    }
    Ok(())
}

fn data_size(b: &[u8]) -> usize {
    off_strings(b) + size_strings(b)
}

/// Moves the data after `pos + oldlen` so that the region becomes `newlen`
/// bytes long. New bytes are not initialized.
fn splice(b: &mut [u8], pos: usize, oldlen: usize, newlen: usize) -> FdtResult<()> {
    let end = data_size(b);
    if pos + oldlen > end {
        return err(FDT_ERR_BADOFFSET);
    }
    if end - oldlen + newlen > totalsize(b) || end - oldlen + newlen > b.len() {
        return err(FDT_ERR_NOSPACE);
    }
    b.copy_within(pos + oldlen..end, pos + newlen);
    Ok(())
}

fn splice_mem_rsv(b: &mut [u8], pos: usize, oldn: usize, newn: usize) -> FdtResult<()> {
    splice(b, pos, oldn * RSV_ENTRY_SIZE, newn * RSV_ENTRY_SIZE)?;
    let delta = |v: usize| v + newn * RSV_ENTRY_SIZE - oldn * RSV_ENTRY_SIZE;
    let (s, t) = (delta(off_struct(b)), delta(off_strings(b)));
    set_hdr(b, H_OFF_DT_STRUCT, s);
    set_hdr(b, H_OFF_DT_STRINGS, t);
    Ok(())
}

fn splice_struct(b: &mut [u8], pos: usize, oldlen: usize, newlen: usize) -> FdtResult<()> {
    splice(b, pos, oldlen, newlen)?;
    let (s, t) = (size_struct(b) + newlen - oldlen, off_strings(b) + newlen - oldlen);
    set_hdr(b, H_SIZE_DT_STRUCT, s);
    set_hdr(b, H_OFF_DT_STRINGS, t);
    Ok(())
}

fn splice_string(b: &mut [u8], newlen: usize) -> FdtResult<usize> {
    let pos = off_strings(b) + size_strings(b);
    splice(b, pos, 0, newlen)?;
    let s = size_strings(b) + newlen;
    set_hdr(b, H_SIZE_DT_STRINGS, s);
    Ok(pos)
}

/// Finds `s` in the strings block or appends it. Returns the string offset
/// and whether it was appended.
fn find_add_string(b: &mut [u8], s: &[u8]) -> FdtResult<(usize, bool)> {
    let tab = off_strings(b);
    let size = size_strings(b);
    let len = s.len() + 1;
    if len <= size {
        for p in 0..=size - len {
            let cand = &b[tab + p..tab + p + len];
            if &cand[..s.len()] == s && cand[s.len()] == 0 {
                return Ok((p, false));
            }
        }
    }
    let pos = splice_string(b, len)?;
    b[pos..pos + s.len()].copy_from_slice(s);
    b[pos + s.len()] = 0;
    Ok((pos - tab, true))
}

fn del_last_string(b: &mut [u8], s: &[u8]) {
    let size = size_strings(b) - (s.len() + 1);
    set_hdr(b, H_SIZE_DT_STRINGS, size);
}

fn resize_property(b: &mut [u8], node: i32, name: &[u8], len: usize) -> FdtResult<usize> {
    let (prop, data, oldlen) = get_property(b, node, name)?;
    splice_struct(b, data, tag_align(oldlen), tag_align(len))?;
    let abs = off_struct(b) + prop as usize;
    set_be32(b, abs + 4, len as u32);
    Ok(data)
}

fn add_property(b: &mut [u8], node: i32, name: &[u8], len: usize) -> FdtResult<usize> {
    let next = check_node_offset(b, node)?;
    let (nameoff, allocated) = find_add_string(b, name)?;
    let abs = off_struct(b) + next;
    if let Err(e) = splice_struct(b, abs, 0, PROP_HEADER_SIZE + tag_align(len)) {
        if allocated {
            del_last_string(b, name);
        }
        return Err(e);
    }
    set_be32(b, abs, FDT_PROP);
    set_be32(b, abs + 4, len as u32);
    set_be32(b, abs + 8, nameoff as u32);
    Ok(abs + PROP_HEADER_SIZE)
}

/// Equivalent of `fdt_setprop_placeholder()`: makes room for a `len` byte
/// value and returns its absolute offset in the blob.
pub fn setprop_placeholder(b: &mut [u8], node: i32, name: &[u8], len: usize) -> FdtResult<usize> {
    rw_probe(b)?;
    let pos = match resize_property(b, node, name, len) {
        Err(e) if e == -FDT_ERR_NOTFOUND => add_property(b, node, name, len)?,
        r => r?,
    };
    // Clear the alignment padding so that the blob stays deterministic.
    b[pos + len..pos + tag_align(len)].fill(0);
    Ok(pos)
}

/// Equivalent of `fdt_setprop()`.
pub fn setprop(b: &mut [u8], node: i32, name: &[u8], val: &[u8]) -> FdtResult<()> {
    let pos = setprop_placeholder(b, node, name, val.len())?;
    b[pos..pos + val.len()].copy_from_slice(val);
    Ok(())
}

/// Equivalent of `fdt_setprop_string()`.
pub fn setprop_string(b: &mut [u8], node: i32, name: &[u8], s: &[u8]) -> FdtResult<()> {
    let pos = setprop_placeholder(b, node, name, s.len() + 1)?;
    b[pos..pos + s.len()].copy_from_slice(s);
    b[pos + s.len()] = 0;
    Ok(())
}

/// Equivalent of `fdt_setprop_u32()`.
pub fn setprop_u32(b: &mut [u8], node: i32, name: &[u8], val: u32) -> FdtResult<()> {
    setprop(b, node, name, &val.to_be_bytes())
}

/// Equivalent of `fdt_delprop()`.
pub fn delprop(b: &mut [u8], node: i32, name: &[u8]) -> FdtResult<()> {
    rw_probe(b)?;
    let (prop, _, len) = get_property(b, node, name)?;
    let abs = off_struct(b) + prop as usize;
    splice_struct(b, abs, PROP_HEADER_SIZE + tag_align(len), 0)
}

/// Equivalent of `fdt_setprop_inplace_namelen_partial()`.
pub fn setprop_inplace_partial(b: &mut [u8], node: i32, name: &[u8], idx: usize, val: &[u8]) -> FdtResult<()> {
    let (_, data, len) = get_property(b, node, name)?;
    if idx + val.len() > len {
        return err(FDT_ERR_NOSPACE);
    }
    b[data + idx..data + idx + val.len()].copy_from_slice(val);
    Ok(())
}

/// Equivalent of `fdt_setprop_inplace()`.
pub fn setprop_inplace(b: &mut [u8], node: i32, name: &[u8], val: &[u8]) -> FdtResult<()> {
    let (_, _, len) = get_property(b, node, name)?;
    if len != val.len() {
        return err(FDT_ERR_NOSPACE);
    }
    setprop_inplace_partial(b, node, name, 0, val)
}

/// Equivalent of `fdt_nop_property()`.
pub fn nop_property(b: &mut [u8], node: i32, name: &[u8]) -> FdtResult<()> {
    let (prop, _, len) = get_property(b, node, name)?;
    let abs = off_struct(b) + prop as usize;
    for off in (abs..abs + PROP_HEADER_SIZE + tag_align(len)).step_by(TAG_SIZE) {
        set_be32(b, off, FDT_NOP);
    }
    Ok(())
}

fn node_end_offset(b: &[u8], node: i32) -> FdtResult<usize> {
    let mut depth = 0;
    let mut offset = node;
    while depth >= 0 {
        offset = next_node(b, offset, Some(&mut depth))?;
    }
    Ok(offset as usize)
}

/// Equivalent of `fdt_del_node()`.
pub fn del_node(b: &mut [u8], node: i32) -> FdtResult<()> {
    rw_probe(b)?;
    let end = node_end_offset(b, node)?;
    let abs = off_struct(b) + node as usize;
    splice_struct(b, abs, end - node as usize, 0)
}

/// Equivalent of `fdt_add_subnode_namelen()`.
pub fn add_subnode(b: &mut [u8], parent: i32, name: &[u8]) -> FdtResult<i32> {
    rw_probe(b)?;
    match subnode_offset(b, parent, name) {
        Ok(_) => return err(FDT_ERR_EXISTS),
        Err(e) if e != -FDT_ERR_NOTFOUND => return Err(e),
        Err(_) => {}
    }
    // Place the new node after the parent's properties.
    let (_, mut next) = next_tag(b, check_node_offset(b, parent).map(|_| parent as usize)?)?;
    let offset = loop {
        let offset = next;
        let (tag, n) = next_tag(b, offset)?;
        next = n;
        if tag != FDT_PROP && tag != FDT_NOP {
            break offset;
        }
    };
    let namelen = tag_align(name.len() + 1);
    let nodelen = TAG_SIZE + namelen + TAG_SIZE;
    let abs = off_struct(b) + offset;
    splice_struct(b, abs, 0, nodelen)?;
    set_be32(b, abs, FDT_BEGIN_NODE);
    b[abs + TAG_SIZE..abs + TAG_SIZE + namelen].fill(0);
    b[abs + TAG_SIZE..abs + TAG_SIZE + name.len()].copy_from_slice(name);
    set_be32(b, abs + nodelen - TAG_SIZE, FDT_END_NODE);
    Ok(offset as i32)
}

/// Equivalent of `fdt_set_name()`.
pub fn set_name(b: &mut [u8], node: i32, name: &[u8]) -> FdtResult<()> {
    rw_probe(b)?;
    let oldlen = get_name(b, node)?.len();
    let abs = off_struct(b) + node as usize + TAG_SIZE;
    let newlen = tag_align(name.len() + 1);
    splice_struct(b, abs, tag_align(oldlen + 1), newlen)?;
    b[abs..abs + newlen].fill(0);
    b[abs..abs + name.len()].copy_from_slice(name);
    Ok(())
}

/// Equivalent of `fdt_del_mem_rsv()`.
pub fn del_mem_rsv(b: &mut [u8], n: usize) -> FdtResult<()> {
    rw_probe(b)?;
    if n >= num_mem_rsv(b)? {
        return err(FDT_ERR_NOTFOUND);
    }
    let pos = off_rsvmap(b) + n * RSV_ENTRY_SIZE;
    splice_mem_rsv(b, pos, 1, 0)
}

/// Equivalent of `fdt_open_into()`: lays `src` out into `dst` in the order
/// required by the read-write functions, with `dst.len()` as the new size.
pub fn open_into(src: &[u8], dst: &mut [u8]) -> FdtResult<()> {
    check_header(src)?;
    let rsv = (num_mem_rsv(src)? + 1) * RSV_ENTRY_SIZE;
    let struct_size = size_struct(src);
    let strings_size = size_strings(src);
    let off_rsv = HEADER_SIZE.next_multiple_of(8);
    let needed = off_rsv + rsv + struct_size + strings_size;
    if dst.len() < needed {
        return err(FDT_ERR_NOSPACE);
    }
    let mut out = Vec::with_capacity(needed);
    out.extend_from_slice(&src[..HEADER_SIZE]);
    out.resize(off_rsv, 0);
    out.extend_from_slice(&src[off_rsvmap(src)..off_rsvmap(src) + rsv]);
    out.extend_from_slice(&src[off_struct(src)..off_struct(src) + struct_size]);
    out.extend_from_slice(&src[off_strings(src)..off_strings(src) + strings_size]);
    set_be32(&mut out, H_MAGIC, FDT_MAGIC);
    set_be32(&mut out, H_VERSION, 17);
    set_be32(&mut out, H_LAST_COMP_VERSION, 16);
    set_hdr(&mut out, H_OFF_MEM_RSVMAP, off_rsv);
    set_hdr(&mut out, H_OFF_DT_STRUCT, off_rsv + rsv);
    set_hdr(&mut out, H_OFF_DT_STRINGS, off_rsv + rsv + struct_size);
    set_hdr(&mut out, H_SIZE_DT_STRUCT, struct_size);
    set_hdr(&mut out, H_SIZE_DT_STRINGS, strings_size);
    set_hdr(&mut out, H_TOTALSIZE, dst.len());
    dst[..needed].copy_from_slice(&out);
    Ok(())
}

/// Equivalent of `fdt_pack()`: removes the free space at the end of the blob.
pub fn pack(b: &mut [u8]) -> FdtResult<()> {
    rw_probe(b)?;
    let src = b[..totalsize(b)].to_vec();
    let rsv = (num_mem_rsv(&src)? + 1) * RSV_ENTRY_SIZE;
    let size = HEADER_SIZE.next_multiple_of(8) + rsv + size_struct(&src) + size_strings(&src);
    open_into(&src, &mut b[..size])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A minimal tree: / { compatible = "test"; chosen { }; soc { uart@1000 { }; }; };
    fn sample() -> Vec<u8> {
        let mut st = Vec::new();
        let strings = b"compatible\0".to_vec();
        let push = |st: &mut Vec<u8>, v: u32| st.extend_from_slice(&v.to_be_bytes());
        let name = |st: &mut Vec<u8>, n: &[u8]| {
            st.extend_from_slice(n);
            st.push(0);
            while !st.len().is_multiple_of(4) {
                st.push(0);
            }
        };
        push(&mut st, FDT_BEGIN_NODE);
        name(&mut st, b"");
        push(&mut st, FDT_PROP);
        push(&mut st, 5);
        push(&mut st, 0);
        name(&mut st, b"test");
        push(&mut st, FDT_BEGIN_NODE);
        name(&mut st, b"chosen");
        push(&mut st, FDT_END_NODE);
        push(&mut st, FDT_BEGIN_NODE);
        name(&mut st, b"soc");
        push(&mut st, FDT_BEGIN_NODE);
        name(&mut st, b"uart@1000");
        push(&mut st, FDT_END_NODE);
        push(&mut st, FDT_END_NODE);
        push(&mut st, FDT_END_NODE);
        push(&mut st, FDT_END);

        let mut b = vec![0u8; HEADER_SIZE + RSV_ENTRY_SIZE];
        b.extend_from_slice(&st);
        b.extend_from_slice(&strings);
        let total = b.len();
        set_be32(&mut b, H_MAGIC, FDT_MAGIC);
        set_hdr(&mut b, H_TOTALSIZE, total);
        set_hdr(&mut b, H_OFF_MEM_RSVMAP, HEADER_SIZE);
        set_hdr(&mut b, H_OFF_DT_STRUCT, HEADER_SIZE + RSV_ENTRY_SIZE);
        set_hdr(&mut b, H_OFF_DT_STRINGS, HEADER_SIZE + RSV_ENTRY_SIZE + st.len());
        set_hdr(&mut b, H_VERSION, 17);
        set_hdr(&mut b, H_LAST_COMP_VERSION, 16);
        set_hdr(&mut b, H_SIZE_DT_STRINGS, strings.len());
        set_hdr(&mut b, H_SIZE_DT_STRUCT, st.len());
        b
    }

    fn enlarged(b: &[u8], extra: usize) -> Vec<u8> {
        let mut out = vec![0u8; b.len() + extra];
        open_into(b, &mut out).unwrap();
        out
    }

    #[test]
    fn lookup() {
        let b = sample();
        check_header(&b).unwrap();
        let uart = path_offset(&b, b"/soc/uart").unwrap();
        assert_eq!(get_name(&b, uart).unwrap(), b"uart@1000");
        assert_eq!(get_path(&b, uart).unwrap(), b"/soc/uart@1000");
        assert_eq!(get_path(&b, 0).unwrap(), b"/");
        assert_eq!(getprop(&b, 0, b"compatible").unwrap(), b"test\0");
        assert_eq!(path_offset(&b, b"/nope"), Err(-FDT_ERR_NOTFOUND));
        assert_eq!(node_offset_by_compatible(&b, -1, b"test"), Ok(0));
    }

    #[test]
    fn edit_and_pack() {
        let mut b = enlarged(&sample(), 512);
        let soc = path_offset(&b, b"/soc").unwrap();
        let node = add_subnode(&mut b, soc, b"virtio@a000").unwrap();
        setprop_string(&mut b, node, b"compatible", b"virtio,mmio").unwrap();
        setprop_u32(&mut b, node, b"interrupts", 42).unwrap();
        assert_eq!(add_subnode(&mut b, soc, b"virtio@a000"), Err(-FDT_ERR_EXISTS));

        let chosen = path_offset(&b, b"/chosen").unwrap();
        setprop_string(&mut b, chosen, b"bootargs", b"console=ttyAMA0").unwrap();
        setprop_string(&mut b, chosen, b"bootargs", b"root=/dev/vda").unwrap();
        let uart = path_offset(&b, b"/soc/uart").unwrap();
        set_name(&mut b, uart, b"serial@0").unwrap();
        let serial = path_offset(&b, b"/soc/serial@0").unwrap();
        del_node(&mut b, serial).unwrap();
        pack(&mut b).unwrap();
        let size = totalsize(&b);
        let b = &b[..size];

        check_header(b).unwrap();
        let chosen = path_offset(b, b"/chosen").unwrap();
        assert_eq!(getprop(b, chosen, b"bootargs").unwrap(), b"root=/dev/vda\0");
        let node = path_offset(b, b"/soc/virtio@a000").unwrap();
        assert_eq!(getprop(b, node, b"interrupts").unwrap(), 42u32.to_be_bytes());
        assert_eq!(node_offset_by_compatible(b, 0, b"virtio,mmio"), Ok(node));
        assert_eq!(path_offset(b, b"/soc/serial"), Err(-FDT_ERR_NOTFOUND));
        assert_eq!(subnodes(b, path_offset(b, b"/soc").unwrap()), [node]);
    }

    #[test]
    fn delete_and_nop() {
        let mut b = enlarged(&sample(), 128);
        delprop(&mut b, 0, b"compatible").unwrap();
        assert_eq!(getprop(&b, 0, b"compatible"), Err(-FDT_ERR_NOTFOUND));
        setprop_string(&mut b, 0, b"model", b"x").unwrap();
        nop_property(&mut b, 0, b"model").unwrap();
        assert_eq!(getprop(&b, 0, b"model"), Err(-FDT_ERR_NOTFOUND));
        assert!(path_offset(&b, b"/soc/uart@1000").is_ok());
    }

    #[test]
    fn no_space() {
        let mut b = sample();
        assert_eq!(setprop_string(&mut b, 0, b"model", b"too big"), Err(-FDT_ERR_NOSPACE));
    }
}
