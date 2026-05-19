//! macOS-only system audio capture via ScreenCaptureKit.
//!
//! ScreenCaptureKit on macOS 13/14 has no true audio-only mode — we must
//! configure a (tiny) video stream and silently drop the frames. macOS 15
//! finally exposes a microphone-via-SCK path that we don't yet use (cpal
//! handles mic in `capture.rs`).
//!
//! Both layouts ScreenCaptureKit can deliver are handled: a single
//! interleaved buffer with N channels, or N planar buffers with 1 channel
//! each. We downmix to mono inside the SCStream callback so the rest of
//! the pipeline only ever sees mono Float32 @ 48 kHz.

#![cfg(target_os = "macos")]

use screencapturekit::prelude::*;
use screencapturekit::{AudioBuffer, AudioBufferList};
use std::sync::{Arc, Mutex};

const SAMPLE_RATE_HZ: u32 = 48_000;
// SCK seems to prefer stereo; we ask for 2 and downmix to mono ourselves.
const SCK_CHANNELS: u32 = 2;
// Apple-side quirk: SCStreamConfiguration validates dimensions even when
// the consumer only cares about audio. Smallest sane resolution that doesn't
// crash modern macOS versions — keeps GPU work minimal.
const DUMMY_VIDEO_WIDTH: u32 = 16;
const DUMMY_VIDEO_HEIGHT: u32 = 16;

struct AudioCallback {
    samples: Arc<Mutex<Vec<f32>>>,
}

impl SCStreamOutputTrait for AudioCallback {
    fn did_output_sample_buffer(&self, sample: CMSampleBuffer, of_type: SCStreamOutputType) {
        if of_type != SCStreamOutputType::Audio {
            // We registered for Audio only, but be defensive.
            return;
        }
        let Some(audio_list) = sample.audio_buffer_list() else {
            return;
        };

        let mut samples = self.samples.lock().unwrap();
        append_downmixed(&mut samples, &audio_list);
    }
}

/// Discards screen frames silently. SCK still allocates GPU work for the
/// tiny dummy stream; this handler keeps the pipeline from buffering frames
/// indefinitely on the SCK side.
struct DiscardScreenCallback;

impl SCStreamOutputTrait for DiscardScreenCallback {
    fn did_output_sample_buffer(&self, _sample: CMSampleBuffer, _of_type: SCStreamOutputType) {
        // intentional: drop the frame
    }
}

fn append_downmixed(out: &mut Vec<f32>, audio_list: &AudioBufferList) {
    let buffers: Vec<&AudioBuffer> = audio_list.iter().collect();
    if buffers.is_empty() {
        return;
    }
    const BYTES_PER_SAMPLE: usize = 4; // Float32

    if buffers.len() == 1 {
        // Single buffer — either mono or interleaved multi-channel.
        let buf = buffers[0];
        let channels = buf.number_channels.max(1) as usize;
        let bytes = buf.data();
        let total_samples = bytes.len() / BYTES_PER_SAMPLE;
        let frames = total_samples / channels;
        if frames == 0 {
            return;
        }
        // SAFETY: SCK delivers Float32; data() returns a properly aligned
        // pointer because CoreMedia allocates AudioBuffer payloads aligned
        // to 16 bytes.
        let floats: &[f32] = unsafe {
            std::slice::from_raw_parts(bytes.as_ptr().cast::<f32>(), total_samples)
        };
        if channels == 1 {
            out.extend_from_slice(floats);
        } else {
            out.reserve(frames);
            for frame_idx in 0..frames {
                let mut sum = 0.0_f32;
                for ch in 0..channels {
                    sum += floats[frame_idx * channels + ch];
                }
                out.push(sum / channels as f32);
            }
        }
    } else {
        // Planar — one buffer per channel.
        let per_channel: Vec<&[f32]> = buffers
            .iter()
            .map(|b| {
                let bytes = b.data();
                unsafe {
                    std::slice::from_raw_parts(
                        bytes.as_ptr().cast::<f32>(),
                        bytes.len() / BYTES_PER_SAMPLE,
                    )
                }
            })
            .collect();
        let min_len = per_channel.iter().map(|c| c.len()).min().unwrap_or(0);
        if min_len == 0 {
            return;
        }
        out.reserve(min_len);
        for i in 0..min_len {
            let mut sum = 0.0_f32;
            for ch in &per_channel {
                sum += ch[i];
            }
            out.push(sum / per_channel.len() as f32);
        }
    }
}

/// Sample rate of the captured/downmixed system audio. Always 48 kHz to
/// match cpal mic config and the Opus encoder.
#[allow(dead_code)]
pub const SYSTEM_AUDIO_SAMPLE_RATE: u32 = SAMPLE_RATE_HZ;

pub struct SystemAudioRecorder {
    stream: Option<SCStream>,
    samples: Arc<Mutex<Vec<f32>>>,
}

impl SystemAudioRecorder {
    pub fn new() -> Self {
        Self {
            stream: None,
            samples: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn start(&mut self) -> Result<(), String> {
        if self.stream.is_some() {
            return Err("system audio recording already in progress".into());
        }
        self.samples.lock().unwrap().clear();

        let content = SCShareableContent::get()
            .map_err(|e| format!("SCShareableContent::get (Screen Recording permission?): {e}"))?;
        let display = content
            .displays()
            .into_iter()
            .next()
            .ok_or("no displays available for ScreenCaptureKit")?;

        let filter = SCContentFilter::create()
            .with_display(&display)
            .with_excluding_windows(&[])
            .build();

        let config = SCStreamConfiguration::new()
            .with_width(DUMMY_VIDEO_WIDTH)
            .with_height(DUMMY_VIDEO_HEIGHT)
            .with_pixel_format(PixelFormat::BGRA)
            .with_shows_cursor(false)
            .with_captures_audio(true)
            .with_sample_rate(SAMPLE_RATE_HZ as i32)
            .with_channel_count(SCK_CHANNELS as i32);

        let mut stream = SCStream::new(&filter, &config);
        let audio_handler = AudioCallback {
            samples: Arc::clone(&self.samples),
        };
        stream.add_output_handler(audio_handler, SCStreamOutputType::Audio);
        stream.add_output_handler(DiscardScreenCallback, SCStreamOutputType::Screen);

        stream
            .start_capture()
            .map_err(|e| format!("SCStream::start_capture: {e}"))?;

        self.stream = Some(stream);
        Ok(())
    }

    /// Stop the SCK stream and return the captured mono Float32 @ 48 kHz
    /// samples. Returns Ok with whatever was captured even on stop errors
    /// because the user's audio data is precious.
    pub fn stop(&mut self) -> Result<Vec<f32>, String> {
        let stream = self.stream.take();
        if let Some(s) = &stream {
            if let Err(e) = s.stop_capture() {
                eprintln!("[steno] SCStream::stop_capture warning: {e}");
            }
        }
        drop(stream);
        let samples = std::mem::take(&mut *self.samples.lock().unwrap());
        Ok(samples)
    }
}
