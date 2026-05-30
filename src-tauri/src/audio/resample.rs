//! Mono PCM resampler.
//!
//! For the M1.5 fallback path: devices that don't natively run at 48 kHz get
//! their captured samples resampled here at `stop()` time. Quality is linear
//! interpolation — adequate for speech (the only signal source the spec
//! cares about) at the small rate ratios that show up in practice
//! (44.1k → 48k, 96k → 48k). Higher quality is a `rubato`-shaped swap-in
//! when M2 transcription accuracy benchmarks justify it.
//!
//! Caller is mono — pre-downmix happens in the recorder.

const TARGET_HZ: u32 = 48_000;

/// Resample mono `input` from `src_rate` to 48 kHz. Returns the input
/// untouched when rates already match or input is empty.
pub fn resample_mono_to_48k(input: &[f32], src_rate: u32) -> Vec<f32> {
    if input.is_empty() || src_rate == TARGET_HZ {
        return input.to_vec();
    }
    let ratio = src_rate as f64 / TARGET_HZ as f64;
    let out_len = ((input.len() as f64 / ratio).floor() as usize).max(1);
    let last_idx = input.len() - 1;
    let mut out = Vec::with_capacity(out_len);
    for i in 0..out_len {
        let src_pos = i as f64 * ratio;
        let lo = (src_pos.floor() as usize).min(last_idx);
        let hi = (lo + 1).min(last_idx);
        let frac = (src_pos - lo as f64) as f32;
        out.push(input[lo] * (1.0 - frac) + input[hi] * frac);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passthrough_when_rates_match() {
        let input = vec![0.1, 0.2, 0.3];
        let out = resample_mono_to_48k(&input, TARGET_HZ);
        assert_eq!(out, input);
    }

    #[test]
    fn empty_input_returns_empty() {
        let out = resample_mono_to_48k(&[], 44_100);
        assert!(out.is_empty());
    }

    #[test]
    fn downsamples_96k_to_48k() {
        let input = vec![0.0, 1.0, 0.0, -1.0, 0.0, 1.0, 0.0, -1.0];
        let out = resample_mono_to_48k(&input, 96_000);
        // 8 / (96000/48000) = 4 samples expected
        assert_eq!(out.len(), 4);
    }

    #[test]
    fn upsamples_44100_to_48k_increases_count() {
        let input = vec![0.0_f32; 4410]; // 100 ms at 44.1 kHz
        let out = resample_mono_to_48k(&input, 44_100);
        // Expected ≈ 4410 * (48000/44100) ≈ 4800
        assert!(out.len() >= 4790 && out.len() <= 4810, "got {}", out.len());
    }

    #[test]
    fn linear_interp_midpoint() {
        // Two-sample input @ rate that lands an output sample exactly between.
        // input at src=2 Hz: [1.0, 3.0]. Target 4 Hz → ratio 0.5.
        // out[0] = input[0] (frac 0)
        // out[1] = input[0]*0.5 + input[1]*0.5 = 2.0
        // out[2] = input[1] (frac 0, clamped)
        let out = resample_mono_to_48k(&[1.0, 3.0], TARGET_HZ / 2);
        // Generic check: midpoint should be 2.0 when present
        if out.len() >= 2 {
            // out[1] expected ≈ 2.0
            assert!((out[1] - 2.0).abs() < 1e-5, "got {:?}", out);
        }
    }
}
