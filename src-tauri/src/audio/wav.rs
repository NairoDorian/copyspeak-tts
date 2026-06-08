// WAV file parsing, PCM sample reading, amplitude envelope extraction, and concatenation.

use super::AmplitudeEnvelope;

/// WAV file format information extracted from the header
pub(super) struct WavInfo {
    pub sample_rate: u32,
    pub channels: u16,
    pub bits_per_sample: u16,
    pub data_offset: usize,
    pub data_size: usize,
}

/// Parse WAV header to extract format information
pub(super) fn parse_wav_header(bytes: &[u8]) -> Result<WavInfo, String> {
    if bytes.is_empty() {
        return Err("Audio file is empty".to_string());
    }

    if bytes.len() < 44 {
        return Err(format!(
            "Audio file too small ({} bytes). A valid WAV file requires at least 44 bytes. The file is corrupted or incomplete.",
            bytes.len()
        ));
    }

    // Check RIFF header
    if &bytes[0..4] != b"RIFF" {
        return Err("Invalid audio format: not a valid WAV file (missing RIFF header). The file may be corrupted or in the wrong format.".to_string());
    }

    // Check WAVE format
    if &bytes[8..12] != b"WAVE" {
        return Err("Invalid audio format: not a valid WAV file (missing WAVE header). The file may be corrupted or in the wrong format.".to_string());
    }

    // Find fmt chunk
    let mut offset = 12;
    let mut sample_rate = 0u32;
    let mut channels = 0u16;
    let mut bits_per_sample = 0u16;
    let mut data_offset = 0usize;
    let mut data_size = 0usize;

    while offset + 8 <= bytes.len() {
        let chunk_id = &bytes[offset..offset + 4];
        let chunk_size = u32::from_le_bytes([
            bytes[offset + 4],
            bytes[offset + 5],
            bytes[offset + 6],
            bytes[offset + 7],
        ]) as usize;

        if chunk_id == b"fmt " {
            if offset + 8 + chunk_size > bytes.len() {
                return Err("fmt chunk extends beyond file".to_string());
            }

            // Parse fmt chunk (minimum 16 bytes)
            if chunk_size < 16 {
                return Err("fmt chunk too small".to_string());
            }

            let audio_format = u16::from_le_bytes([bytes[offset + 8], bytes[offset + 9]]);
            if audio_format != 1 {
                return Err(format!("Unsupported audio format: {}", audio_format));
            }

            channels = u16::from_le_bytes([bytes[offset + 10], bytes[offset + 11]]);
            sample_rate = u32::from_le_bytes([
                bytes[offset + 12],
                bytes[offset + 13],
                bytes[offset + 14],
                bytes[offset + 15],
            ]);
            bits_per_sample = u16::from_le_bytes([bytes[offset + 22], bytes[offset + 23]]);
        } else if chunk_id == b"data" {
            data_offset = offset + 8;
            data_size = chunk_size;
        }

        offset += 8 + chunk_size;
    }

    if sample_rate == 0 || channels == 0 || bits_per_sample == 0 {
        return Err(format!(
            "Invalid WAV format: missing required format information (sample_rate: {}, channels: {}, bits_per_sample: {}). The file is corrupted.",
            sample_rate, channels, bits_per_sample
        ));
    }

    if data_offset == 0 {
        return Err(
            "Invalid WAV file: no audio data chunk found. The file is corrupted or incomplete."
                .to_string(),
        );
    }

    // Validate format parameters
    if channels == 0 || channels > 8 {
        return Err(format!(
            "Invalid audio format: unsupported channel count ({}). Only 1-8 channels are supported.",
            channels
        ));
    }

    if bits_per_sample != 8
        && bits_per_sample != 16
        && bits_per_sample != 24
        && bits_per_sample != 32
    {
        return Err(format!(
            "Unsupported audio format: {}-bit audio is not supported. Only 8, 16, 24, and 32-bit audio is supported.",
            bits_per_sample
        ));
    }

    if !(8000..=192000).contains(&sample_rate) {
        return Err(format!(
            "Invalid sample rate: {} Hz. Valid range is 8000-192000 Hz.",
            sample_rate
        ));
    }

    Ok(WavInfo {
        sample_rate,
        channels,
        bits_per_sample,
        data_offset,
        data_size,
    })
}

/// Extract an amplitude envelope from raw audio bytes.
/// Returns `num_bars` normalized RMS values (0.0–1.0).
///
/// Computed in a single pass over the PCM data — no intermediate Vec<f32> allocation.
/// For long audio, decimates samples to keep computation fast.
/// For non-WAV formats (MP3, etc.), returns a default envelope since we can't
/// easily parse those formats without full decoding.
pub fn extract_envelope(audio_bytes: &[u8], num_bars: usize) -> Result<AmplitudeEnvelope, String> {
    if audio_bytes.len() < 12 || &audio_bytes[0..4] != b"RIFF" || &audio_bytes[8..12] != b"WAVE" {
        let estimated_duration_ms = if audio_bytes.len() > 100 {
            if audio_bytes[0] == 0xFF && (audio_bytes[1] & 0xE0) == 0xE0 {
                ((audio_bytes.len() as f64 / 16000.0) * 1000.0) as u64
            } else {
                2000
            }
        } else {
            1000
        };
        log::debug!(
            "Non-WAV audio detected, using default envelope with estimated duration: {}ms",
            estimated_duration_ms
        );
        return Ok(AmplitudeEnvelope {
            values: vec![0.5; num_bars],
            duration_ms: estimated_duration_ms,
        });
    }

    let wav_info = parse_wav_header(audio_bytes)?;

    let data = &audio_bytes[wav_info.data_offset..wav_info.data_offset + wav_info.data_size];
    let bytes_per_sample = (wav_info.bits_per_sample / 8) as usize;
    let channels = wav_info.channels as usize;
    let frame_size = bytes_per_sample * channels;
    let total_frames = data.len() / frame_size;

    if total_frames == 0 {
        return Ok(AmplitudeEnvelope {
            values: vec![0.0; num_bars],
            duration_ms: 0,
        });
    }

    // Decimate: for long audio, stride through frames to avoid processing every sample.
    // Target at most num_bars * 256 samples per bar for good resolution.
    let target_per_bar = 256usize;
    let stride = (total_frames / (num_bars * target_per_bar)).max(1);
    let frames_per_bar = total_frames / num_bars;

    let mut max_rms = 0.0f32;
    let mut rms_values = Vec::with_capacity(num_bars);

    for bar in 0..num_bars {
        let bar_start = bar * frames_per_bar / stride * stride;
        let bar_end = if bar == num_bars - 1 { total_frames } else { (bar + 1) * frames_per_bar / stride * stride };

        let mut sum_sq = 0.0f64;
        let mut count = 0u64;

        let mut frame_idx = bar_start;
        while frame_idx < bar_end {
            let offset = frame_idx * frame_size;
            if offset + frame_size > data.len() {
                break;
            }
            let mono: f32 = decode_frame_mono(&data[offset..], bytes_per_sample, channels);
            sum_sq += (mono as f64) * (mono as f64);
            count += 1;
            frame_idx += stride;
        }

        let rms = if count > 0 {
            (sum_sq / count as f64).sqrt() as f32
        } else {
            0.0
        };
        if rms > max_rms {
            max_rms = rms;
        }
        rms_values.push(rms);
    }

    let duration_ms = (total_frames as f64 / wav_info.sample_rate as f64 * 1000.0) as u64;

    let normalized: Vec<f32> = if max_rms > 0.0 {
        rms_values.iter().map(|&v| v / max_rms).collect()
    } else {
        vec![0.0; num_bars]
    };

    Ok(AmplitudeEnvelope {
        values: normalized,
        duration_ms,
    })
}

/// Decode a single multi-channel PCM frame to mono f32, inline, no heap allocation.
#[inline(always)]
#[allow(clippy::needless_range_loop)]
fn decode_frame_mono(frame: &[u8], bytes_per_sample: usize, channels: usize) -> f32 {
    match bytes_per_sample {
        2 => {
            let mut sum = 0.0f32;
            for ch in 0..channels {
                let off = ch * 2;
                let s = i16::from_le_bytes([frame[off], frame[off + 1]]);
                sum += s as f32 / 32768.0;
            }
            sum / channels as f32
        }
        1 => {
            let mut sum = 0.0f32;
            for ch in 0..channels {
                let s = frame[ch] as i16 - 128;
                sum += s as f32 / 128.0;
            }
            sum / channels as f32
        }
        3 => {
            let mut sum = 0.0f32;
            for ch in 0..channels {
                let off = ch * 3;
                let s = i32::from_le_bytes([frame[off], frame[off + 1], frame[off + 2], 0])
                    << 8
                    >> 8;
                sum += s as f32 / 8388608.0;
            }
            sum / channels as f32
        }
        4 => {
            let mut sum = 0.0f32;
            for ch in 0..channels {
                let off = ch * 4;
                let s = i32::from_le_bytes([
                    frame[off],
                    frame[off + 1],
                    frame[off + 2],
                    frame[off + 3],
                ]);
                sum += s as f32 / 2147483648.0;
            }
            sum / channels as f32
        }
        _ => 0.0,
    }
}

/// Find the start of the "data" chunk payload in a WAV file.
/// Returns the byte offset where raw PCM data begins (after "data" + 4-byte size field).
pub(super) fn find_wav_data_offset(wav: &[u8]) -> Option<usize> {
    if wav.len() < 12 || &wav[0..4] != b"RIFF" || &wav[8..12] != b"WAVE" {
        return None;
    }
    let mut pos = 12usize;
    while pos + 8 <= wav.len() {
        let chunk_id = &wav[pos..pos + 4];
        let chunk_size =
            u32::from_le_bytes([wav[pos + 4], wav[pos + 5], wav[pos + 6], wav[pos + 7]]) as usize;
        if chunk_id == b"data" {
            return Some(pos + 8); // byte offset right after "data" + 4-byte size
        }
        pos += 8 + chunk_size;
        if !chunk_size.is_multiple_of(2) {
            pos += 1;
        } // WAV chunks are word-aligned
    }
    None
}

/// Concatenate multiple PCM WAV buffers into a single valid WAV.
/// All buffers must share the same sample rate, channels, and bit depth.
/// Returns the first buffer unchanged if the slice has only one element.
pub fn concat_wav_files(wavs: Vec<Vec<u8>>) -> Result<Vec<u8>, String> {
    match wavs.len() {
        0 => return Err("No audio fragments to concatenate".to_string()),
        1 => return Ok(wavs.into_iter().next().unwrap()),
        _ => {}
    }

    let first = &wavs[0];
    let first_data_offset =
        find_wav_data_offset(first).ok_or("First audio fragment is not a valid WAV file")?;

    // Everything before the "data" chunk identifier (RIFF header + fmt chunk + other chunks)
    let prefix_end = first_data_offset - 8; // back up past "data" (4) + data_size (4)

    // Collect raw PCM from all fragments
    let mut all_pcm: Vec<u8> = first[first_data_offset..].to_vec();
    for (idx, wav) in wavs[1..].iter().enumerate() {
        match find_wav_data_offset(wav) {
            Some(offset) => all_pcm.extend_from_slice(&wav[offset..]),
            None => log::warn!("[Audio] Fragment {} is not a valid WAV, skipping", idx + 1),
        }
    }

    // Build combined WAV
    let mut output: Vec<u8> = Vec::with_capacity(prefix_end + 8 + all_pcm.len());
    output.extend_from_slice(b"RIFF");
    output.extend_from_slice(&[0u8; 4]); // RIFF size placeholder
    output.extend_from_slice(&first[8..prefix_end]); // "WAVE" + fmt chunk
    output.extend_from_slice(b"data");
    output.extend_from_slice(&(all_pcm.len() as u32).to_le_bytes());
    output.extend_from_slice(&all_pcm);

    // Fix RIFF chunk size (total file size - 8 for the "RIFF" + size fields)
    let riff_size = (output.len() - 8) as u32;
    output[4..8].copy_from_slice(&riff_size.to_le_bytes());

    log::debug!(
        "[Audio] Concatenated {} WAV fragments into {} bytes",
        wavs.len(),
        output.len()
    );

    Ok(output)
}
