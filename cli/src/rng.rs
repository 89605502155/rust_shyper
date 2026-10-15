//! Seeds the random number generator of the MVM kernel.
//!
//! The MVM kernel (Linux 4.9) has no hardware RNG driver for QEMU and gathers
//! entropy only from interrupts, so its CRNG may stay uninitialized for a
//! long time. `getrandom(2)` blocks until then, and so does everything using
//! TLS (curl, dnf). Like haveged, this collects CPU timing jitter and credits
//! it to the kernel with the RNDADDENTROPY ioctl.

use std::{
    fs::{File, OpenOptions},
    hint::black_box,
    io::Read,
    os::raw::c_int,
    time::Instant,
};

use rustix::ioctl::{ioctl, opcode, Opcode, Setter};

const RANDOM_DEV: &str = "/dev/random";
const ENTROPY_AVAIL: &str = "/proc/sys/kernel/random/entropy_avail";
const RNDADDENTROPY: Opcode = opcode::write::<[c_int; 2]>(b'R', 0x03);

/// Timing samples per batch; one bit of entropy is credited per 16 samples.
const SAMPLES: usize = 4096;
const WORDS: usize = SAMPLES / 4;
const CREDIT_BITS: c_int = (SAMPLES / 16) as c_int;
/// The kernel considers the CRNG ready at 128 bits; seed until this many are
/// available, but at most `MAX_BATCHES` times.
const TARGET_BITS: u32 = 256;
const MAX_BATCHES: usize = 16;

/// `struct rand_pool_info` with a fixed-size buffer.
#[repr(C)]
struct RandPoolInfo {
    entropy_count: c_int,
    buf_size: c_int,
    buf: [u32; WORDS],
}

/// Low bytes of the run time of a small memory walk, repeated `SAMPLES`
/// times. The run time varies with caches, interrupts and the host.
fn jitter(scratch: &mut [u8]) -> [u32; WORDS] {
    let mut words = [0u32; WORDS];
    let mut prev = Instant::now();
    for i in 0..SAMPLES {
        for j in (0..scratch.len()).step_by(64) {
            scratch[j] = black_box(scratch[j].wrapping_add(i as u8));
        }
        let now = Instant::now();
        let delta = now.duration_since(prev).as_nanos() as u32;
        prev = now;
        words[i / 4] |= (delta & 0xff) << (8 * (i % 4));
    }
    words
}

fn entropy_avail() -> u32 {
    std::fs::read_to_string(ENTROPY_AVAIL)
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0)
}

fn seed() -> Result<u32, String> {
    let dev = OpenOptions::new()
        .write(true)
        .open(RANDOM_DEV)
        .map_err(|err| format!("open {}: {}", RANDOM_DEV, err))?;
    let mut scratch = vec![0u8; 64 * 1024];
    for _ in 0..MAX_BATCHES {
        if entropy_avail() >= TARGET_BITS {
            break;
        }
        let mut buf = jitter(&mut scratch);
        // Mixing in the current pool output does not hurt and adds nothing
        // to the credit.
        let mut extra = [0u8; 4 * WORDS];
        if File::open("/dev/urandom")
            .and_then(|mut f| f.read_exact(&mut extra))
            .is_ok()
        {
            for (w, b) in buf.iter_mut().zip(extra.as_chunks::<4>().0) {
                *w ^= u32::from_ne_bytes(*b);
            }
        }
        let info = RandPoolInfo {
            entropy_count: CREDIT_BITS,
            buf_size: (4 * WORDS) as c_int,
            buf,
        };
        // SAFETY: RNDADDENTROPY reads a `struct rand_pool_info` whose buffer
        // is `buf_size` bytes long, which `RandPoolInfo` is.
        unsafe { ioctl(&dev, Setter::<RNDADDENTROPY, RandPoolInfo>::new(info)) }
            .map_err(|err| format!("RNDADDENTROPY: {}", err))?;
    }
    Ok(entropy_avail())
}

pub fn sys_seed_rng() {
    match seed() {
        Ok(bits) => println!("random pool seeded, {} bits available", bits),
        Err(err) => {
            println!("err: seed-rng: {}", err);
            std::process::exit(1);
        }
    }
}
