//! Atomic peak-level meter shared between an audio callback (writer) and
//! the UI-facing meter thread (reader/resetter).
//!
//! `observe(&samples)` is RT-safe: no allocation, single atomic fetch-max.
//! `take_peak()` reads and atomically resets to zero — so the UI thread
//! reports a fresh peak per polling interval rather than a monotonic max.
//!
//! Float-to-bits monotonicity: for `f >= 0.0`, the IEEE 754 bit ordering
//! matches the numeric ordering, so `fetch_max` over `f32::to_bits` gives
//! the max-value semantics we want. The callback always submits
//! `|sample|` which is non-negative, so the invariant holds.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

#[derive(Clone, Default)]
pub struct Meter {
    peak_bits: Arc<AtomicU32>,
}

impl Meter {
    pub fn new() -> Self {
        Self::default()
    }

    /// Update the stored peak with the max absolute value in `samples`.
    /// No-op if the local max is zero. Safe to call from RT audio
    /// callbacks.
    pub fn observe(&self, samples: &[f32]) {
        let mut local_peak: f32 = 0.0;
        for &s in samples {
            let a = s.abs();
            if a > local_peak {
                local_peak = a;
            }
        }
        if local_peak == 0.0 {
            return;
        }
        let _ = self.peak_bits.fetch_max(local_peak.to_bits(), Ordering::Relaxed);
    }

    /// Read current peak and atomically reset to 0.
    pub fn take_peak(&self) -> f32 {
        let bits = self.peak_bits.swap(0, Ordering::Relaxed);
        f32::from_bits(bits)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_does_not_change_peak() {
        let m = Meter::new();
        m.observe(&[]);
        assert_eq!(m.take_peak(), 0.0);
    }

    #[test]
    fn observes_max_abs() {
        let m = Meter::new();
        m.observe(&[0.1, -0.5, 0.3, -0.2]);
        assert_eq!(m.take_peak(), 0.5);
    }

    #[test]
    fn take_resets_to_zero() {
        let m = Meter::new();
        m.observe(&[0.7]);
        assert_eq!(m.take_peak(), 0.7);
        assert_eq!(m.take_peak(), 0.0);
    }

    #[test]
    fn observe_accumulates_max_until_take() {
        let m = Meter::new();
        m.observe(&[0.3]);
        m.observe(&[0.1]);
        m.observe(&[0.8]);
        assert_eq!(m.take_peak(), 0.8);
    }
}
