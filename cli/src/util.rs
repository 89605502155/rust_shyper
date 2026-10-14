// Copyright (c) 2023 Beihang University, Huawei Technologies Co.,Ltd. All rights reserved.
// Rust-Shyper is licensed under Mulan PSL v2.
// You can use this software according to the terms and conditions of the Mulan PSL v2.
// You may obtain a copy of Mulan PSL v2 at:
//          http://license.coscl.org.cn/MulanPSL2
// THIS SOFTWARE IS PROVIDED ON AN "AS IS" BASIS, WITHOUT WARRANTIES OF ANY KIND,
// EITHER EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO NON-INFRINGEMENT,
// MERCHANTABILITY OR FIT FOR A PARTICULAR PURPOSE.
// See the Mulan PSL v2 for more details.

use std::fs::File;

pub fn file_size(path: &String) -> Result<u64, String> {
    let file = File::open(path).map_err(|_| format!("Open file {} err", path))?;
    let metadata = file.metadata().map_err(|_| format!("Get file {} metadata err", path))?;
    Ok(metadata.len())
}

/// The string in a NUL terminated byte array.
pub fn cstr_arr_to_string(buf: &[u8]) -> String {
    let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf8_lossy(&buf[..len]).into_owned()
}

/// `s` as a NUL terminated byte array, truncated to fit.
pub fn string_to_cstr_arr(s: String) -> [u8; 32] {
    let mut buf: [u8; 32] = [0; 32];
    let len = s.len().min(buf.len() - 1);
    buf[..len].copy_from_slice(&s.as_bytes()[..len]);
    buf
}

/// Writes and reads back every byte of `cache`. This also makes the kernel
/// back the whole buffer with memory before its physical address is used.
pub fn check_cache_address(cache: &mut [u8]) -> Result<(), String> {
    for (i, b) in cache.iter_mut().enumerate() {
        *b = (i % 128) as u8;
    }
    match cache.iter().enumerate().position(|(i, &b)| b != (i % 128) as u8) {
        Some(i) => Err(format!("check_cache_address: Mismatch at {} offset", i)),
        None => Ok(()),
    }
}

pub fn string_to_u64(s: String) -> Result<u64, String> {
    let s = s.trim().to_string();
    if s.starts_with("0x") {
        match u64::from_str_radix(&s[2..], 16) {
            Ok(num) => Ok(num),
            Err(_) => Err(format!("Not hex string: {}", s)),
        }
    } else if s.starts_with("0b") {
        match u64::from_str_radix(&s[2..], 2) {
            Ok(num) => Ok(num),
            Err(_) => Err(format!("Not binary string: {}", s)),
        }
    } else {
        // must be decimal
        match u64::from_str_radix(&s, 10) {
            Ok(num) => Ok(num),
            Err(_) => Err(format!("String {} is not in hex/bin/decimal format!", s)),
        }
    }
}
