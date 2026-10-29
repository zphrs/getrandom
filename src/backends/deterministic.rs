//! Deterministic testing backend — seeded ChaCha12 RNG, single-thread only.

// This module should only be compiled under `cfg(test)`, so `std` is always
// linked even though the crate is `#![no_std]`.
extern crate std;

pub use crate::util::{inner_u32, inner_u64};

use crate::Error;

use chacha20::ChaCha12Rng;
use core::mem::MaybeUninit;
use rand_core::{Rng, SeedableRng};
use std::{
    collections::{HashMap, hash_map::Entry},
    sync::{Mutex, OnceLock},
};

/// The RNG, initialised exactly once on first use.
static RNG: OnceLock<Mutex<HashMap<std::thread::ThreadId, ChaCha12Rng>>> = OnceLock::new();

#[inline]
pub fn fill_inner(dest: &mut [MaybeUninit<u8>]) -> Result<(), Error> {
    // get current thread id
    let id = std::thread::current().id();

    let rng = RNG.get_or_init(|| HashMap::new().into());

    let mut guard = rng.lock().unwrap();

    let entry = guard
        .entry(id)
        .or_insert_with(|| ChaCha12Rng::from_seed([42u8; 32]));

    // SAFETY: `fill_bytes` fully overwrites every byte of the slice, so
    // treating uninitialized `MaybeUninit<u8>` as `u8` for the purpose of
    // writing (never reading) is sound.
    let dest_init = unsafe { dest.assume_init_mut() };
    entry.fill_bytes(dest_init);

    Ok(())
}


/// Seeds the calling thread's RNG with `seed` instead of the default
/// `[42; 32]`, so that threads which would otherwise draw identical streams
/// draw independent ones. Threads that never call this keep the default seed.
///
/// # Panics
///
/// Panics if the calling thread has already drawn from (or seeded) its RNG.
pub fn set_thread_seed(seed: u64) {
    let id = std::thread::current().id();
    let rng = RNG.get_or_init(|| HashMap::new().into());
    let seeded = match rng.lock().unwrap().entry(id) {
        Entry::Occupied(_) => false,
        Entry::Vacant(entry) => {
            entry.insert(ChaCha12Rng::seed_from_u64(seed));
            true
        }
    };
    // panics only once the lock is released, so it isn't poisoned
    assert!(
        seeded,
        "set_thread_seed must be called before the thread's first draw"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deterministic() {
        let mut buf = [0u8; 32];
        crate::fill(&mut buf).unwrap();
        assert_eq!(
            [
                0x1b, 0x8c, 0x20, 0xcd, 0xe2, 0xdb, 0xb4, 0x3c, 0xd3, 0xc7, 0x9, 0xb2, 0x90, 0xac,
                0x50, 0xdc, 0xd2, 0xbe, 0x2a, 0x87, 0xa3, 0xa2, 0x45, 0x44, 0xb5, 0xa5, 0x10, 0x9b,
                0xc7, 0x6e, 0xa7, 0xfb,
            ],
            buf
        );
    }
    use std::thread;
    #[test]
    fn test_deterministic_multithread() {
        let jh = thread::spawn(|| {
            let mut buf = [0u8; 32];
            crate::fill(&mut buf).unwrap();
            assert_eq!(
                [
                    0x1b, 0x8c, 0x20, 0xcd, 0xe2, 0xdb, 0xb4, 0x3c, 0xd3, 0xc7, 0x9, 0xb2, 0x90,
                    0xac, 0x50, 0xdc, 0xd2, 0xbe, 0x2a, 0x87, 0xa3, 0xa2, 0x45, 0x44, 0xb5, 0xa5,
                    0x10, 0x9b, 0xc7, 0x6e, 0xa7, 0xfb,
                ],
                buf
            );
        });
        let jh2 = thread::spawn(|| {
            let mut buf = [0u8; 32];
            crate::fill(&mut buf).unwrap();
            assert_eq!(
                [
                    0x1b, 0x8c, 0x20, 0xcd, 0xe2, 0xdb, 0xb4, 0x3c, 0xd3, 0xc7, 0x9, 0xb2, 0x90,
                    0xac, 0x50, 0xdc, 0xd2, 0xbe, 0x2a, 0x87, 0xa3, 0xa2, 0x45, 0x44, 0xb5, 0xa5,
                    0x10, 0x9b, 0xc7, 0x6e, 0xa7, 0xfb,
                ],
                buf
            );
        });

        jh.join().unwrap();
        jh2.join().unwrap();
    }

    #[test]
    fn test_set_thread_seed() {
        fn draw(seed: Option<u64>) -> [u8; 32] {
            thread::spawn(move || {
                if let Some(seed) = seed {
                    set_thread_seed(seed);
                }
                let mut buf = [0u8; 32];
                crate::fill(&mut buf).unwrap();
                buf
            })
            .join()
            .unwrap()
        }
        let (a, b) = (draw(Some(1)), draw(Some(2)));
        assert_ne!(a, b, "different seeds draw different streams");
        assert_eq!(a, draw(Some(1)), "same seed, same stream");
        assert_ne!(a, draw(None), "seeded differs from the default");
    }

    #[test]
    #[should_panic(expected = "before the thread's first draw")]
    fn test_set_thread_seed_after_draw_panics() {
        thread::spawn(|| {
            let mut buf = [0u8; 1];
            crate::fill(&mut buf).unwrap();
            set_thread_seed(1);
        })
        .join()
        .map_err(std::panic::resume_unwind)
        .unwrap();
    }
}
