// Copyright (c) 2023 Beihang University, Huawei Technologies Co.,Ltd. All rights reserved.
// Rust-Shyper is licensed under Mulan PSL v2.
// You can use this software according to the terms and conditions of the Mulan PSL v2.
// You may obtain a copy of Mulan PSL v2 at:
//          http://license.coscl.org.cn/MulanPSL2
// THIS SOFTWARE IS PROVIDED ON AN "AS IS" BASIS, WITHOUT WARRANTIES OF ANY KIND,
// EITHER EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO NON-INFRINGEMENT,
// MERCHANTABILITY OR FIT FOR A PARTICULAR PURPOSE.
// See the Mulan PSL v2 for more details.

//! Pure Rust port of libfdt's device tree overlay support (fdt_overlay.c).

use alloc::vec::Vec;

use crate::raw::*;

const fn err<T>(code: i32) -> FdtResult<T> {
    Err(-code)
}

/// Target node of a fragment in `fdt`, and the `target-path` if one is used.
fn target_offset(fdt: &[u8], fdto: &[u8], fragment: i32) -> FdtResult<(i32, Option<Vec<u8>>)> {
    let phandle = match getprop(fdto, fragment, b"target") {
        Ok(v) if v.len() == 4 && be32(v, 0) != u32::MAX => be32(v, 0),
        Ok(_) => return err(FDT_ERR_BADPHANDLE),
        Err(_) => 0,
    };
    if phandle != 0 {
        return Ok((node_offset_by_phandle(fdt, phandle)?, None));
    }
    let path = getprop(fdto, fragment, b"target-path").map_err(|e| {
        if e == -FDT_ERR_NOTFOUND {
            -FDT_ERR_BADOVERLAY
        } else {
            e
        }
    })?;
    let path = path.split(|&c| c == 0).next().unwrap_or(&[]).to_vec();
    Ok((path_offset(fdt, &path)?, Some(path)))
}

fn phandle_add_offset(fdto: &mut [u8], node: i32, name: &[u8], delta: u32) -> FdtResult<()> {
    let val = getprop(fdto, node, name)?;
    if val.len() != 4 {
        return err(FDT_ERR_BADPHANDLE);
    }
    let adj = be32(val, 0).checked_add(delta).ok_or(-FDT_ERR_NOPHANDLES)?;
    if adj == u32::MAX {
        return err(FDT_ERR_NOPHANDLES);
    }
    setprop_inplace(fdto, node, name, &adj.to_be_bytes())
}

fn adjust_node_phandles(fdto: &mut [u8], node: i32, delta: u32) -> FdtResult<()> {
    for name in [&b"phandle"[..], b"linux,phandle"] {
        match phandle_add_offset(fdto, node, name, delta) {
            Err(e) if e != -FDT_ERR_NOTFOUND => return Err(e),
            _ => {}
        }
    }
    for child in subnodes(fdto, node) {
        adjust_node_phandles(fdto, child, delta)?;
    }
    Ok(())
}

fn update_local_node_references(fdto: &mut [u8], tree: i32, fixup: i32, delta: u32) -> FdtResult<()> {
    for prop in properties(fdto, fixup) {
        let (name, data, len) = get_property_by_offset(fdto, prop)?;
        let name = name.to_vec();
        if len % 4 != 0 {
            return err(FDT_ERR_BADOVERLAY);
        }
        let offsets: Vec<u32> = (0..len / 4).map(|i| be32(fdto, data + 4 * i)).collect();
        let tree_val =
            getprop(fdto, tree, &name).map_err(|e| if e == -FDT_ERR_NOTFOUND { -FDT_ERR_BADOVERLAY } else { e })?;
        let tree_val = tree_val.to_vec();
        for poffset in offsets {
            let poffset = poffset as usize;
            if poffset + 4 > tree_val.len() {
                return err(FDT_ERR_BADOVERLAY);
            }
            let adj = be32(&tree_val, poffset).wrapping_add(delta);
            setprop_inplace_partial(fdto, tree, &name, poffset, &adj.to_be_bytes()).map_err(|e| {
                if e == -FDT_ERR_NOSPACE {
                    -FDT_ERR_BADOVERLAY
                } else {
                    e
                }
            })?;
        }
    }
    for fixup_child in subnodes(fdto, fixup) {
        let name = get_name(fdto, fixup_child)?.to_vec();
        let tree_child =
            subnode_offset(fdto, tree, &name).map_err(
                |e| {
                    if e == -FDT_ERR_NOTFOUND {
                        -FDT_ERR_BADOVERLAY
                    } else {
                        e
                    }
                },
            )?;
        update_local_node_references(fdto, tree_child, fixup_child, delta)?;
    }
    Ok(())
}

fn update_local_references(fdto: &mut [u8], delta: u32) -> FdtResult<()> {
    match path_offset(fdto, b"/__local_fixups__") {
        Ok(fixups) => update_local_node_references(fdto, 0, fixups, delta),
        Err(e) if e == -FDT_ERR_NOTFOUND => Ok(()),
        Err(e) => Err(e),
    }
}

fn fixup_one_phandle(
    fdt: &[u8],
    fdto: &mut [u8],
    symbols: FdtResult<i32>,
    path: &[u8],
    name: &[u8],
    poffset: usize,
    label: &[u8],
) -> FdtResult<()> {
    let symbols = symbols?;
    let symbol_path = getprop(fdt, symbols, label)?;
    let symbol_path = symbol_path.split(|&c| c == 0).next().unwrap_or(&[]);
    let symbol = path_offset(fdt, symbol_path)?;
    let phandle = get_phandle(fdt, symbol);
    if phandle == 0 {
        return err(FDT_ERR_NOTFOUND);
    }
    let fixup = path_offset(fdto, path).map_err(|e| if e == -FDT_ERR_NOTFOUND { -FDT_ERR_BADOVERLAY } else { e })?;
    setprop_inplace_partial(fdto, fixup, name, poffset, &phandle.to_be_bytes())
}

/// Handles one `/__fixups__` property: `label = "path:prop:offset", ...`.
fn fixup_phandle(fdt: &[u8], fdto: &mut [u8], symbols: FdtResult<i32>, prop: i32) -> FdtResult<()> {
    let (label, data, len) = get_property_by_offset(fdto, prop)?;
    let label = label.to_vec();
    let value = fdto[data..data + len].to_vec();
    let mut rest = &value[..];
    loop {
        let end = rest.iter().position(|&c| c == 0).ok_or(-FDT_ERR_BADOVERLAY)?;
        let fixup = &rest[..end];
        rest = &rest[end + 1..];

        let mut parts = fixup.splitn(3, |&c| c == b':');
        let (Some(path), Some(name), Some(poffset)) = (parts.next(), parts.next(), parts.next()) else {
            return err(FDT_ERR_BADOVERLAY);
        };
        if name.is_empty() || poffset.is_empty() || !poffset.iter().all(u8::is_ascii_digit) {
            return err(FDT_ERR_BADOVERLAY);
        }
        let poffset = core::str::from_utf8(poffset)
            .ok()
            .and_then(|s| s.parse::<usize>().ok())
            .ok_or(-FDT_ERR_BADOVERLAY)?;
        fixup_one_phandle(fdt, fdto, symbols, path, name, poffset, &label)?;
        if rest.is_empty() {
            return Ok(());
        }
    }
}

fn fixup_phandles(fdt: &[u8], fdto: &mut [u8]) -> FdtResult<()> {
    let fixups = match path_offset(fdto, b"/__fixups__") {
        Ok(o) => o,
        Err(e) if e == -FDT_ERR_NOTFOUND => return Ok(()),
        Err(e) => return Err(e),
    };
    let symbols = path_offset(fdt, b"/__symbols__");
    if let Err(e) = symbols {
        if e != -FDT_ERR_NOTFOUND {
            return Err(e);
        }
    }
    for prop in properties(fdto, fixups) {
        fixup_phandle(fdt, fdto, symbols, prop)?;
    }
    Ok(())
}

fn apply_node(fdt: &mut [u8], target: i32, fdto: &[u8], node: i32) -> FdtResult<()> {
    for prop in properties(fdto, node) {
        let (name, data, len) = get_property_by_offset(fdto, prop)?;
        setprop(fdt, target, name, &fdto[data..data + len])?;
    }
    for subnode in subnodes(fdto, node) {
        let name = get_name(fdto, subnode)?;
        let nnode =
            match add_subnode(fdt, target, name) {
                Err(e) if e == -FDT_ERR_EXISTS => subnode_offset(fdt, target, name).map_err(|e| {
                    if e == -FDT_ERR_NOTFOUND {
                        -FDT_ERR_INTERNAL
                    } else {
                        e
                    }
                })?,
                r => r?,
            };
        apply_node(fdt, nnode, fdto, subnode)?;
    }
    Ok(())
}

fn merge(fdt: &mut [u8], fdto: &[u8]) -> FdtResult<()> {
    for fragment in subnodes(fdto, 0) {
        let overlay = match subnode_offset(fdto, fragment, b"__overlay__") {
            Ok(o) => o,
            Err(e) if e == -FDT_ERR_NOTFOUND => continue,
            Err(e) => return Err(e),
        };
        let (target, _) = target_offset(fdt, fdto, fragment)?;
        apply_node(fdt, target, fdto, overlay)?;
    }
    Ok(())
}

fn symbol_update(fdt: &mut [u8], fdto: &[u8]) -> FdtResult<()> {
    let Ok(ov_sym) = subnode_offset(fdto, 0, b"__symbols__") else {
        return Ok(());
    };
    let root_sym = match subnode_offset(fdt, 0, b"__symbols__") {
        Err(e) if e == -FDT_ERR_NOTFOUND => add_subnode(fdt, 0, b"__symbols__")?,
        r => r?,
    };
    for prop in properties(fdto, ov_sym) {
        let (name, data, len) = get_property_by_offset(fdto, prop)?;
        let path = &fdto[data..data + len];
        // Must be a string property terminated by a single NUL.
        if len < 1 || path.iter().position(|&c| c == 0) != Some(len - 1) {
            return err(FDT_ERR_BADVALUE);
        }
        let path = &path[..len - 1];
        if path.first() != Some(&b'/') {
            return err(FDT_ERR_BADVALUE);
        }
        // Symbols outside of /<fragment>/__overlay__ do not end up in the tree.
        let Some(slash) = path[1..].iter().position(|&c| c == b'/').map(|i| i + 1) else {
            continue;
        };
        let frag_name = &path[1..slash];
        let rest = &path[slash..];
        let rel_path: &[u8] = if let Some(rel) = rest.strip_prefix(b"/__overlay__/") {
            rel
        } else if rest == b"/__overlay__" {
            b""
        } else {
            continue;
        };
        let fragment = subnode_offset(fdto, 0, frag_name).map_err(|_| -FDT_ERR_BADOVERLAY)?;
        subnode_offset(fdto, fragment, b"__overlay__").map_err(|_| -FDT_ERR_BADOVERLAY)?;
        let (target, target_path) = target_offset(fdt, fdto, fragment)?;
        let mut value = match target_path {
            Some(p) => p,
            None => get_path(fdt, target)?,
        };
        if value == b"/" {
            value.clear();
        }
        value.push(b'/');
        value.extend_from_slice(rel_path);
        value.push(0);
        setprop(fdt, root_sym, name, &value)?;
    }
    Ok(())
}

/// Equivalent of `fdt_overlay_apply()`: merges overlay `fdto` into `fdt`.
///
/// `fdt` must have been laid out with [`open_into`] with enough free space.
/// As in libfdt, `fdto` is consumed (its magic is invalidated) and on error
/// `fdt` is invalidated too.
pub fn overlay_apply(fdt: &mut [u8], fdto: &mut [u8]) -> FdtResult<()> {
    let ret = (|| {
        check_header(fdt)?;
        check_header(fdto)?;
        let delta = find_max_phandle(fdt)?;
        adjust_node_phandles(fdto, 0, delta)?;
        update_local_references(fdto, delta)?;
        fixup_phandles(fdt, fdto)?;
        merge(fdt, fdto)?;
        symbol_update(fdt, fdto)
    })();
    set_magic(fdto, !0);
    if ret.is_err() {
        set_magic(fdt, !0);
    }
    ret
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    struct Node {
        name: &'static str,
        props: Vec<(&'static str, Vec<u8>)>,
        children: Vec<Node>,
    }

    fn node(name: &'static str, props: Vec<(&'static str, Vec<u8>)>, children: Vec<Node>) -> Node {
        Node { name, props, children }
    }

    fn s(v: &str) -> Vec<u8> {
        let mut b = v.as_bytes().to_vec();
        b.push(0);
        b
    }

    fn u32v(v: &[u32]) -> Vec<u8> {
        v.iter().flat_map(|x| x.to_be_bytes()).collect()
    }

    /// Builds a blob with `extra` bytes of free space.
    fn build(root: &Node, extra: usize) -> Vec<u8> {
        fn emit(n: &Node, st: &mut Vec<u8>, strs: &mut Vec<u8>) {
            let pad = |st: &mut Vec<u8>| {
                while !st.len().is_multiple_of(4) {
                    st.push(0)
                }
            };
            st.extend_from_slice(&1u32.to_be_bytes());
            st.extend_from_slice(n.name.as_bytes());
            st.push(0);
            pad(st);
            for (name, val) in &n.props {
                let off = strs.len();
                strs.extend_from_slice(name.as_bytes());
                strs.push(0);
                st.extend_from_slice(&3u32.to_be_bytes());
                st.extend_from_slice(&(val.len() as u32).to_be_bytes());
                st.extend_from_slice(&(off as u32).to_be_bytes());
                st.extend_from_slice(val);
                pad(st);
            }
            for c in &n.children {
                emit(c, st, strs);
            }
            st.extend_from_slice(&2u32.to_be_bytes());
        }
        let (mut st, mut strs) = (Vec::new(), Vec::new());
        emit(root, &mut st, &mut strs);
        st.extend_from_slice(&9u32.to_be_bytes());
        let mut b = vec![0u8; 56];
        let hdr = [
            FDT_MAGIC,
            0,
            56,
            (56 + st.len()) as u32,
            40,
            17,
            16,
            0,
            strs.len() as u32,
            st.len() as u32,
        ];
        for (i, v) in hdr.iter().enumerate() {
            set_be32(&mut b, i * 4, *v);
        }
        b.extend_from_slice(&st);
        b.extend_from_slice(&strs);
        let total = b.len();
        set_be32(&mut b, 4, total as u32);
        let mut out = vec![0u8; total + extra];
        open_into(&b, &mut out).unwrap();
        out
    }

    #[test]
    fn apply_overlay() {
        let base = node(
            "",
            vec![],
            vec![
                node(
                    "soc",
                    vec![],
                    vec![node("uart@1000", vec![("phandle", u32v(&[1]))], vec![])],
                ),
                node("__symbols__", vec![("uart", s("/soc/uart@1000"))], vec![]),
            ],
        );
        let overlay = node(
            "",
            vec![],
            vec![
                node(
                    "fragment@0",
                    vec![("target-path", s("/soc"))],
                    vec![node(
                        "__overlay__",
                        vec![],
                        vec![node(
                            "dev@2000",
                            vec![
                                ("compatible", s("vendor,dev")),
                                ("uart-handle", u32v(&[0xffff_ffff])),
                                ("phandle", u32v(&[1])),
                                ("self", u32v(&[7, 1])),
                            ],
                            vec![],
                        )],
                    )],
                ),
                node(
                    "__fixups__",
                    vec![("uart", s("/fragment@0/__overlay__/dev@2000:uart-handle:0"))],
                    vec![],
                ),
                node(
                    "__local_fixups__",
                    vec![],
                    vec![node(
                        "fragment@0",
                        vec![],
                        vec![node(
                            "__overlay__",
                            vec![],
                            vec![node("dev@2000", vec![("self", u32v(&[4]))], vec![])],
                        )],
                    )],
                ),
                node(
                    "__symbols__",
                    vec![("mydev", s("/fragment@0/__overlay__/dev@2000"))],
                    vec![],
                ),
            ],
        );
        let mut fdt = build(&base, 1024);
        let mut fdto = build(&overlay, 0);
        overlay_apply(&mut fdt, &mut fdto).unwrap();
        pack(&mut fdt).unwrap();
        let fdt = &fdt[..totalsize(&fdt)];

        let dev = path_offset(fdt, b"/soc/dev@2000").unwrap();
        assert_eq!(getprop(fdt, dev, b"compatible").unwrap(), b"vendor,dev\0");
        assert_eq!(getprop(fdt, dev, b"uart-handle").unwrap(), u32v(&[1]));
        assert_eq!(get_phandle(fdt, dev), 2);
        assert_eq!(getprop(fdt, dev, b"self").unwrap(), u32v(&[7, 2]));
        let sym = path_offset(fdt, b"/__symbols__").unwrap();
        assert_eq!(getprop(fdt, sym, b"mydev").unwrap(), b"/soc/dev@2000\0");
        assert_eq!(getprop(fdt, sym, b"uart").unwrap(), b"/soc/uart@1000\0");
        assert_eq!(be32(&fdto, 0), !0);
    }

    #[test]
    fn missing_target() {
        let base = node("", vec![], vec![]);
        let overlay = node(
            "",
            vec![],
            vec![node(
                "fragment@0",
                vec![("target-path", s("/nope"))],
                vec![node("__overlay__", vec![], vec![])],
            )],
        );
        let mut fdt = build(&base, 256);
        let mut fdto = build(&overlay, 0);
        assert_eq!(overlay_apply(&mut fdt, &mut fdto), Err(-FDT_ERR_NOTFOUND));
        assert_eq!(be32(&fdt, 0), !0);
    }
}
