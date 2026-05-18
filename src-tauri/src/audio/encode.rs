use audiopus::coder::Encoder;
use audiopus::{Application, Bitrate, Channels, SampleRate};
use ogg::writing::{PacketWriteEndInfo, PacketWriter};
use std::fs::File;
use std::io::BufWriter;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

const SAMPLE_RATE_HZ: i32 = 48_000;
const FRAME_DURATION_MS: usize = 20;
const SAMPLES_PER_FRAME: usize = (SAMPLE_RATE_HZ as usize / 1000) * FRAME_DURATION_MS; // 960
const MAX_PACKET_BYTES: usize = 4000;
// Pre-skip is the number of samples the decoder should drop from the start of
// playback to compensate for encoder lookahead. 3840 samples (80 ms) is the
// value recommended by RFC 7845 for general-purpose use.
const PRE_SKIP_SAMPLES: u16 = 3840;
const VENDOR: &str = "steno";

pub struct OpusOggWriter {
    writer: PacketWriter<'static, BufWriter<File>>,
    encoder: Encoder,
    serial: u32,
    granule: u64,
    channels: u16,
}

impl OpusOggWriter {
    pub fn new(path: &Path, channels: u16, bitrate_bps: i32) -> Result<Self, String> {
        if channels != 1 && channels != 2 {
            return Err(format!("unsupported channel count for Opus: {channels}"));
        }

        let opus_channels = if channels == 1 {
            Channels::Mono
        } else {
            Channels::Stereo
        };
        let mut encoder = Encoder::new(SampleRate::Hz48000, opus_channels, Application::Voip)
            .map_err(|e| format!("opus encoder new: {e}"))?;
        encoder
            .set_bitrate(Bitrate::BitsPerSecond(bitrate_bps))
            .map_err(|e| format!("opus set_bitrate: {e}"))?;

        let file = File::create(path).map_err(|e| format!("create {}: {e}", path.display()))?;
        let buf = BufWriter::new(file);
        let mut writer = PacketWriter::new(buf);

        let serial = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as u32)
            .unwrap_or(0xDEAD_BEEF);

        // ID header (Opus packet 0) — see RFC 7845 §5.1
        let opus_head = build_opus_head(channels as u8, PRE_SKIP_SAMPLES, SAMPLE_RATE_HZ as u32);
        writer
            .write_packet(opus_head, serial, PacketWriteEndInfo::EndPage, 0)
            .map_err(|e| format!("write OpusHead: {e}"))?;

        // Comment header (Opus packet 1) — see RFC 7845 §5.2
        let opus_tags = build_opus_tags();
        writer
            .write_packet(opus_tags, serial, PacketWriteEndInfo::EndPage, 0)
            .map_err(|e| format!("write OpusTags: {e}"))?;

        Ok(Self {
            writer,
            encoder,
            serial,
            granule: 0,
            channels,
        })
    }

    pub fn encode_pcm(&mut self, samples: &[f32]) -> Result<(), String> {
        let interleaved_frame = SAMPLES_PER_FRAME * self.channels as usize;
        if samples.is_empty() {
            return Ok(());
        }

        let mut input = vec![0.0f32; interleaved_frame];
        let mut packet = vec![0u8; MAX_PACKET_BYTES];

        let total_frames = samples.len().div_ceil(interleaved_frame);

        for frame_idx in 0..total_frames {
            let start = frame_idx * interleaved_frame;
            let end = (start + interleaved_frame).min(samples.len());
            let chunk = &samples[start..end];

            input[..chunk.len()].copy_from_slice(chunk);
            // Zero-pad the tail of the last frame if needed.
            for slot in input[chunk.len()..].iter_mut() {
                *slot = 0.0;
            }

            let bytes_written = self
                .encoder
                .encode_float(&input, &mut packet)
                .map_err(|e| format!("opus encode_float: {e}"))?;

            self.granule += SAMPLES_PER_FRAME as u64;

            let end_info = if frame_idx + 1 == total_frames {
                PacketWriteEndInfo::EndStream
            } else {
                PacketWriteEndInfo::NormalPacket
            };

            self.writer
                .write_packet(
                    packet[..bytes_written].to_vec(),
                    self.serial,
                    end_info,
                    self.granule,
                )
                .map_err(|e| format!("ogg write_packet: {e}"))?;
        }

        Ok(())
    }

    pub fn finalize(self) -> Result<(), String> {
        let buf = self.writer.into_inner();
        buf.into_inner().map_err(|e| format!("flush: {e}"))?;
        Ok(())
    }
}

fn build_opus_head(channels: u8, pre_skip: u16, input_sample_rate: u32) -> Vec<u8> {
    let mut head = Vec::with_capacity(19);
    head.extend_from_slice(b"OpusHead");
    head.push(1); // version
    head.push(channels);
    head.extend_from_slice(&pre_skip.to_le_bytes());
    head.extend_from_slice(&input_sample_rate.to_le_bytes());
    head.extend_from_slice(&0u16.to_le_bytes()); // output gain (Q7.8, 0 = no change)
    head.push(0); // channel mapping family 0 (mono/stereo)
    head
}

fn build_opus_tags() -> Vec<u8> {
    let mut tags = Vec::with_capacity(16 + VENDOR.len());
    tags.extend_from_slice(b"OpusTags");
    tags.extend_from_slice(&(VENDOR.len() as u32).to_le_bytes());
    tags.extend_from_slice(VENDOR.as_bytes());
    tags.extend_from_slice(&0u32.to_le_bytes()); // user comment count = 0
    tags
}
