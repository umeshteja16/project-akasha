//! Turning a decoded audio track into timestamped text.
//!
//! Long recordings are transcribed in windows (10 minutes by default), cut at
//! the quietest moment near each window's end so words are rarely split. This
//! bounds memory (a window is ~38 MB of samples), reports progress between
//! windows and lets a cancelled job stop early.

use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use crate::{
    MediaError,
    decode::{AudioReader, SAMPLE_RATE},
};

/// One stretch of recognised speech. Times are milliseconds from the start of
/// the recording.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Segment {
    pub start_ms: u32,
    pub end_ms: u32,
    pub text: String,
}

/// A speech-to-text model.
pub trait Transcriber: Send + Sync {
    /// Transcribe 16 kHz mono `samples`. Segment times are relative to the
    /// start of `samples`. Should return [`MediaError::Cancelled`] soon after
    /// `control` is cancelled.
    fn transcribe(&self, samples: &[f32], control: &Control) -> Result<Vec<Segment>, MediaError>;

    /// The model's name (stored with the transcript).
    fn name(&self) -> &str;
}

/// Lets the caller stop a running transcription (e.g. on worker shutdown).
#[derive(Debug, Clone, Default)]
pub struct Control {
    cancelled: Arc<AtomicBool>,
}

impl Control {
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }

    /// The shared flag, for callbacks that outlive a borrow.
    pub fn flag(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.cancelled)
    }
}

/// Settings for [`transcribe`].
#[derive(Debug, Clone, Copy)]
pub struct Options {
    /// Audio past this point is not transcribed.
    pub max_duration: Duration,
    /// Audio handed to the model at once.
    pub window: Duration,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            max_duration: Duration::from_secs(120 * 60),
            window: Duration::from_secs(10 * 60),
        }
    }
}

/// The result of [`transcribe`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transcript {
    pub segments: Vec<Segment>,
    /// Length of the audio that was transcribed.
    pub duration_ms: u64,
    /// The recording is longer than [`Options::max_duration`]; the rest was skipped.
    pub truncated: bool,
}

/// Below this RMS a window counts as silence and is not given to the model
/// (Whisper tends to invent text for silence).
const SILENCE_RMS: f32 = 1e-3;
/// Look for a cut point in this much audio before the window's end.
const CUT_SEARCH: usize = SAMPLE_RATE as usize * 10;
/// Loudness is measured over frames of this many samples (100 ms).
const CUT_FRAME: usize = SAMPLE_RATE as usize / 10;

/// Transcribe `reader`'s track with `model`. `progress` gets the fraction done
/// (0–1) after each window when the track length is known.
pub fn transcribe(
    reader: &mut AudioReader,
    model: &dyn Transcriber,
    options: &Options,
    control: &Control,
    progress: &mut dyn FnMut(f32),
) -> Result<Transcript, MediaError> {
    let rate = SAMPLE_RATE as usize;
    let max_samples = (options.max_duration.as_secs_f64() * rate as f64) as usize;
    let window = ((options.window.as_secs_f64() * rate as f64) as usize).max(CUT_SEARCH * 2);
    let expected = reader
        .duration()
        .map(|d| ((d.as_secs_f64() * rate as f64) as usize).min(max_samples))
        .filter(|&n| n > 0);

    let mut segments = Vec::new();
    let mut buffer: Vec<f32> = Vec::new();
    // Samples before `buffer` (already transcribed).
    let mut offset = 0usize;
    let mut ended = false;
    loop {
        if control.is_cancelled() {
            return Err(MediaError::Cancelled);
        }
        let room = max_samples.saturating_sub(offset + buffer.len());
        while !ended && buffer.len() < window && room > 0 {
            let want = (window - buffer.len()).min(max_samples - offset - buffer.len());
            let piece = reader.read(want)?;
            if piece.is_empty() {
                ended = true;
            } else {
                buffer.extend(piece);
            }
            if offset + buffer.len() >= max_samples {
                break;
            }
        }
        if buffer.is_empty() {
            break;
        }
        let at_limit = offset + buffer.len() >= max_samples;
        let cut = if ended || at_limit {
            buffer.len()
        } else {
            quietest_point(&buffer)
        };
        let piece = &buffer[..cut];
        if rms(piece) >= SILENCE_RMS {
            let base = ms(offset);
            for s in model.transcribe(piece, control)? {
                let text = s.text.trim();
                if text.is_empty() {
                    continue;
                }
                let start = base.saturating_add(s.start_ms);
                let end = base.saturating_add(s.end_ms).max(start);
                segments.push(Segment {
                    start_ms: start,
                    end_ms: end.min(ms(offset + cut)),
                    text: text.to_owned(),
                });
            }
        }
        offset += cut;
        buffer.drain(..cut);
        if let Some(total) = expected {
            progress((offset as f32 / total as f32).min(1.0));
        }
        if at_limit && buffer.is_empty() {
            break;
        }
        if ended && buffer.is_empty() {
            break;
        }
    }
    let truncated = !ended && offset >= max_samples && !reader.read(1)?.is_empty();
    Ok(Transcript {
        segments,
        duration_ms: offset as u64 * 1000 / rate as u64,
        truncated,
    })
}

fn ms(samples: usize) -> u32 {
    u32::try_from(samples as u64 * 1000 / u64::from(SAMPLE_RATE)).unwrap_or(u32::MAX)
}

fn rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt()
}

/// Where to end a full window: the start of the quietest 100 ms frame in its
/// last 10 seconds.
fn quietest_point(buffer: &[f32]) -> usize {
    let from = buffer.len().saturating_sub(CUT_SEARCH);
    let mut best = (f32::MAX, buffer.len());
    let mut start = from;
    while start + CUT_FRAME <= buffer.len() {
        let loudness = rms(&buffer[start..start + CUT_FRAME]);
        if loudness < best.0 {
            best = (loudness, start + CUT_FRAME / 2);
        }
        start += CUT_FRAME;
    }
    best.1.max(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cuts_at_the_quiet_part() {
        let mut buffer = vec![0.5f32; SAMPLE_RATE as usize * 30];
        let quiet = SAMPLE_RATE as usize * 25;
        buffer[quiet..quiet + CUT_FRAME].fill(0.0);
        let cut = quietest_point(&buffer);
        assert!(cut >= quiet && cut <= quiet + CUT_FRAME, "{cut}");
    }
}
