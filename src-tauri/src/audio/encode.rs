use ogg::writing::{PacketWriteEndInfo, PacketWriter};
use opus::{Application, Bitrate, Channels, Encoder};
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
    // Streaming state (M1.5):
    // partial holds samples that don't yet make a full 20 ms frame; they
    // get flushed during the next feed() once the buffer crosses the
    // frame-size threshold, or padded with silence in finalize().
    partial: Vec<f32>,
    // The most-recently encoded packet is held back so finalize() can
    // mark it as EndStream (the OGG EOS flag lives on the page-end info
    // for the LAST packet; we can't retroactively flip it once written).
    pending: Option<(Vec<u8>, u64)>,
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
        let mut encoder = Encoder::new(SAMPLE_RATE_HZ as u32, opus_channels, Application::Voip)
            .map_err(|e| format!("opus encoder new: {e}"))?;
        encoder
            .set_bitrate(Bitrate::Bits(bitrate_bps))
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
            partial: Vec::new(),
            pending: None,
        })
    }

    /// Stream `samples` into the encoder. Whole-frame multiples get
    /// encoded immediately; a sub-frame remainder is buffered for the
    /// next call. The most recent encoded packet is always held back so
    /// `finalize()` can mark the true final packet with the EndStream
    /// page flag.
    pub fn feed(&mut self, samples: &[f32]) -> Result<(), String> {
        if samples.is_empty() {
            return Ok(());
        }
        let interleaved_frame = SAMPLES_PER_FRAME * self.channels as usize;
        let mut packet = vec![0u8; MAX_PACKET_BYTES];

        self.partial.extend_from_slice(samples);

        while self.partial.len() >= interleaved_frame {
            let frame: Vec<f32> = self.partial.drain(..interleaved_frame).collect();
            let bytes_written = self
                .encoder
                .encode_float(&frame, &mut packet)
                .map_err(|e| format!("opus encode_float: {e}"))?;
            self.granule += SAMPLES_PER_FRAME as u64;
            self.flush_pending_as_normal()?;
            self.pending = Some((packet[..bytes_written].to_vec(), self.granule));
        }
        Ok(())
    }

    fn flush_pending_as_normal(&mut self) -> Result<(), String> {
        if let Some((bytes, granule)) = self.pending.take() {
            self.writer
                .write_packet(bytes, self.serial, PacketWriteEndInfo::NormalPacket, granule)
                .map_err(|e| format!("ogg write_packet: {e}"))?;
        }
        Ok(())
    }

    /// Drain the partial-frame buffer, write the final EndStream-marked
    /// packet, and close the file. Consumes self so callers can't keep
    /// using a finalized writer.
    pub fn finalize(mut self) -> Result<(), String> {
        let interleaved_frame = SAMPLES_PER_FRAME * self.channels as usize;
        let mut packet = vec![0u8; MAX_PACKET_BYTES];

        if !self.partial.is_empty() {
            // Encode the (zero-padded) remainder as the new "last packet";
            // the prior pending packet (if any) is no longer last.
            let mut input = vec![0.0f32; interleaved_frame];
            input[..self.partial.len()].copy_from_slice(&self.partial);
            self.partial.clear();
            let bytes_written = self
                .encoder
                .encode_float(&input, &mut packet)
                .map_err(|e| format!("opus encode_float (final partial): {e}"))?;
            self.granule += SAMPLES_PER_FRAME as u64;
            self.flush_pending_as_normal()?;
            self.pending = Some((packet[..bytes_written].to_vec(), self.granule));
        } else if self.pending.is_none() {
            // Nothing was ever fed — synthesize a silent frame so the
            // file has at least one Opus packet with the EOS marker.
            let input = vec![0.0f32; interleaved_frame];
            let bytes_written = self
                .encoder
                .encode_float(&input, &mut packet)
                .map_err(|e| format!("opus encode_float (silence): {e}"))?;
            self.granule += SAMPLES_PER_FRAME as u64;
            self.pending = Some((packet[..bytes_written].to_vec(), self.granule));
        }

        // pending is guaranteed Some at this point.
        let (bytes, granule) = self.pending.take().expect("pending packet for EOS");
        self.writer
            .write_packet(bytes, self.serial, PacketWriteEndInfo::EndStream, granule)
            .map_err(|e| format!("ogg write final EOS packet: {e}"))?;

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
