// Copyright (c) 2026 Andrey Ferubko.
// trosna-shyper is licensed under Mulan PSL v2.
// You can use this software according to the terms and conditions of the Mulan PSL v2.
// You may obtain a copy of Mulan PSL v2 at:
//          http://license.coscl.org.cn/MulanPSL2
// THIS SOFTWARE IS PROVIDED ON AN "AS IS" BASIS, WITHOUT WARRANTIES OF ANY KIND,
// EITHER EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO NON-INFRINGEMENT,
// MERCHANTABILITY OR FIT FOR A PARTICULAR PURPOSE.
// See the Mulan PSL v2 for more details.

//! `shyper vm console`: an interactive terminal on a guest VM console.
//!
//! The virtio console of a guest VM shows up in the MVM as a hvc device. Both
//! the device and the user's terminal are switched to raw mode and bytes are
//! copied in both directions until the escape key is pressed.

use std::{
    fs::OpenOptions,
    io::{self, Read, Write},
    os::fd::AsFd,
    thread,
};

use rustix::termios::{tcgetattr, tcsetattr, OptionalActions, Termios};

/// Ctrl-] leaves the console, as in telnet.
const ESCAPE: u8 = 0x1d;

/// Puts a terminal in raw mode and restores it when dropped.
struct RawMode<F: AsFd> {
    fd: F,
    saved: Termios,
}

impl<F: AsFd> RawMode<F> {
    fn new(fd: F) -> io::Result<Self> {
        let saved = tcgetattr(&fd)?;
        let mut raw = saved.clone();
        raw.make_raw();
        tcsetattr(&fd, OptionalActions::Now, &raw)?;
        Ok(Self { fd, saved })
    }
}

impl<F: AsFd> Drop for RawMode<F> {
    fn drop(&mut self) {
        let _ = tcsetattr(&self.fd, OptionalActions::Now, &self.saved);
    }
}

/// Connects the terminal to the guest console `device` until Ctrl-].
pub fn vm_console(device: &str) -> io::Result<()> {
    let dev = OpenOptions::new().read(true).write(true).open(device)?;
    // Without raw mode the MVM tty would echo the guest output back to it.
    let _dev_raw = RawMode::new(dev.try_clone()?)?;

    println!("Connected to {device}, press Ctrl-] to leave.\r");
    let stdin = io::stdin();
    // Not a terminal (input from a pipe): the input is just forwarded.
    let _term_raw = RawMode::new(stdin.as_fd()).ok();

    let mut from_guest = dev.try_clone()?;
    thread::spawn(move || {
        let mut out = io::stdout();
        let mut buf = [0u8; 4096];
        while let Ok(n) = from_guest.read(&mut buf) {
            if n == 0 || out.write_all(&buf[..n]).and_then(|_| out.flush()).is_err() {
                break;
            }
        }
    });

    let mut to_guest = dev;
    let mut input = stdin.lock();
    let mut buf = [0u8; 1024];
    loop {
        let n = input.read(&mut buf)?;
        if n == 0 {
            break;
        }
        let data = &buf[..n];
        if let Some(pos) = data.iter().position(|&b| b == ESCAPE) {
            to_guest.write_all(&data[..pos])?;
            break;
        }
        to_guest.write_all(data)?;
    }
    println!("\r\nDisconnected from {device}.\r");
    Ok(())
}
