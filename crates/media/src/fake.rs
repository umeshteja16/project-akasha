//! A deterministic stand-in for a speech model, for tests and development.
//!
//! It "hears" tones: every second of audio louder than a whisper becomes the
//! words "tone N hertz", N being the pitch estimated from zero crossings and
//! rounded to 10 Hz. Equal neighbouring seconds are merged into one segment. So
//! a generated sine-wave file transcribes to predictable, searchable text with
//! real timestamps, exercising decoding and resampling end to end.

use crate::{
    MediaError,
    decode::SAMPLE_RATE,
    transcribe::{Control, Segment, Transcriber},
};

/// The fake model's name (`AKASHA_WHISPER_MODEL=fake`).
pub const FAKE_MODEL: &str = "fake";

/// Quieter seconds are treated as silence.
const LOUD_ENOUGH: f32 = 0.01;

#[derive(Debug, Default, Clone, Copy)]
pub struct ToneTranscriber;

impl Transcriber for ToneTranscriber {
    fn transcribe(&self, samples: &[f32], control: &Control) -> Result<Vec<Segment>, MediaError> {
        let second = SAMPLE_RATE as usize;
        let mut segments: Vec<Segment> = Vec::new();
        for (i, block) in samples.chunks(second).enumerate() {
            if control.is_cancelled() {
                return Err(MediaError::Cancelled);
            }
            let start_ms = u32::try_from(i * 1000).unwrap_or(u32::MAX);
            let end_ms = start_ms.saturating_add(ms(block.len()));
            let Some(text) = describe(block) else {
                continue;
            };
            match segments.last_mut() {
                Some(last) if last.text == text && last.end_ms == start_ms => last.end_ms = end_ms,
                _ => segments.push(Segment {
                    start_ms,
                    end_ms,
                    text,
                }),
            }
        }
        Ok(segments)
    }

    fn name(&self) -> &str {
        FAKE_MODEL
    }
}

fn ms(samples: usize) -> u32 {
    u32::try_from(samples * 1000 / SAMPLE_RATE as usize).unwrap_or(u32::MAX)
}

fn describe(block: &[f32]) -> Option<String> {
    // Partial blocks shorter than a quarter second are too short to judge.
    if block.len() < SAMPLE_RATE as usize / 4 {
        return None;
    }
    let rms = (block.iter().map(|s| s * s).sum::<f32>() / block.len() as f32).sqrt();
    if rms < LOUD_ENOUGH {
        return None;
    }
    // Pitch from the loud part only, so a second that is half tone, half
    // silence still reads as the tone.
    let frame = SAMPLE_RATE as usize / 100;
    let loud: usize = block
        .chunks(frame)
        .filter(|f| (f.iter().map(|s| s * s).sum::<f32>() / f.len() as f32).sqrt() >= LOUD_ENOUGH)
        .map(<[f32]>::len)
        .sum::<usize>()
        .max(1);
    let crossings = block
        .windows(2)
        .filter(|w| (w[0] < 0.0) != (w[1] < 0.0))
        .count();
    let seconds = loud as f64 / f64::from(SAMPLE_RATE);
    let hertz = (crossings as f64 / 2.0 / seconds / 10.0).round() * 10.0;
    Some(format!("tone {hertz} hertz"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(freq: f64, seconds: f64) -> Vec<f32> {
        let n = (f64::from(SAMPLE_RATE) * seconds) as usize;
        (0..n)
            .map(|i| {
                (0.5 * (2.0 * std::f64::consts::PI * freq * i as f64 / f64::from(SAMPLE_RATE))
                    .sin()) as f32
            })
            .collect()
    }

    #[test]
    fn hears_tones_and_skips_silence() {
        let mut audio = tone(440.0, 2.0);
        audio.extend(vec![0.0; SAMPLE_RATE as usize]);
        audio.extend(tone(880.0, 1.5));
        let segments = ToneTranscriber
            .transcribe(&audio, &Control::default())
            .expect("transcribe");
        let got: Vec<_> = segments
            .iter()
            .map(|s| (s.start_ms, s.end_ms, s.text.as_str()))
            .collect();
        assert_eq!(
            got,
            [(0, 2000, "tone 440 hertz"), (3000, 4500, "tone 880 hertz")]
        );
    }
}
