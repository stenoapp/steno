//! Simple sum-and-halve mixer for two mono streams at the same sample rate.
//!
//! Both inputs are expected to be Float32 mono PCM @ 48 kHz. The shorter
//! stream is silence-padded to the length of the longer one. This is the
//! M1.3-quality mix; tighter sync (timestamp-aligned, drift-compensating)
//! lands later when we get to longer meetings in production.

pub fn mix_mono(mic: &[f32], system: &[f32]) -> Vec<f32> {
    let len = mic.len().max(system.len());
    let mut out = Vec::with_capacity(len);
    for i in 0..len {
        let m = mic.get(i).copied().unwrap_or(0.0);
        let s = system.get(i).copied().unwrap_or(0.0);
        // 0.5 mix avoids hard-clipping when both streams are near full
        // scale. Listeners can mentally compensate; we sacrifice a small
        // amount of headroom for safety. Soft-knee compression is a polish
        // pass.
        out.push(0.5 * (m + s));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn equal_length_streams() {
        let a = [1.0, 0.5, -0.5];
        let b = [0.0, 0.5, 0.5];
        let mix = mix_mono(&a, &b);
        assert_eq!(mix, vec![0.5, 0.5, 0.0]);
    }

    #[test]
    fn mic_longer_than_system() {
        let mic = [1.0, 1.0, 1.0, 1.0];
        let sys = [0.0, 0.0];
        let mix = mix_mono(&mic, &sys);
        assert_eq!(mix, vec![0.5, 0.5, 0.5, 0.5]);
    }

    #[test]
    fn system_longer_than_mic() {
        let mic = [0.0];
        let sys = [1.0, 1.0, 1.0];
        let mix = mix_mono(&mic, &sys);
        assert_eq!(mix, vec![0.5, 0.5, 0.5]);
    }

    #[test]
    fn empty_streams() {
        let mix = mix_mono(&[], &[]);
        assert!(mix.is_empty());
    }
}
