//! Decoding real container/codec combinations and synthesised WAV files, and the
//! windowed transcription pipeline with the deterministic tone "model".
//!
//! The fixtures are two seconds each (440 Hz, then 880 Hz), made with ffmpeg:
//! MP3, Ogg Opus (stereo), M4A (AAC), Ogg Vorbis, MP4 (AAC next to an MPEG-4 video
//! track) and WebM (Opus next to a VP8 video track).

use std::time::Duration;

use akasha_media::{
    AudioReader, Control, MediaError, Options, SAMPLE_RATE, ToneTranscriber, decode_all, transcribe,
};

/// A 16-bit PCM WAV file with the given (interleaved) channels.
fn wav(rate: u32, channels: u16, samples: &[f32]) -> Vec<u8> {
    let data_len = (samples.len() * 2) as u32;
    let mut out = Vec::new();
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&channels.to_le_bytes());
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&(rate * u32::from(channels) * 2).to_le_bytes());
    out.extend_from_slice(&(channels * 2).to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for s in samples {
        out.extend_from_slice(&((s.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes());
    }
    out
}

fn tone(freq: f64, rate: u32, seconds: f64) -> Vec<f32> {
    let n = (f64::from(rate) * seconds) as usize;
    (0..n)
        .map(|i| {
            (0.5 * (2.0 * std::f64::consts::PI * freq * i as f64 / f64::from(rate)).sin()) as f32
        })
        .collect()
}

fn texts(bytes: Vec<u8>, mime: &str, options: &Options) -> Vec<(u32, u32, String)> {
    let mut reader = AudioReader::open(Box::new(std::io::Cursor::new(bytes)), mime).expect("open");
    let t = transcribe(
        &mut reader,
        &ToneTranscriber,
        options,
        &Control::default(),
        &mut |_| {},
    )
    .expect("transcribe");
    t.segments
        .into_iter()
        .map(|s| (s.start_ms, s.end_ms, s.text))
        .collect()
}

#[test]
fn stereo_44k_wav_becomes_16k_mono() {
    let left = tone(440.0, 44_100, 1.0);
    let interleaved: Vec<f32> = left.iter().flat_map(|&s| [s, s]).collect();
    let samples = decode_all(wav(44_100, 2, &interleaved), "audio/wav").expect("decode");
    assert_eq!(samples.len(), SAMPLE_RATE as usize);
    let peak = samples[1000..15_000]
        .iter()
        .fold(0.0f32, |m, s| m.max(s.abs()));
    assert!((peak - 0.5).abs() < 0.03, "peak {peak}");
}

#[test]
fn synthesised_recording_is_transcribed_with_timestamps() {
    let mut audio = tone(440.0, 48_000, 3.0);
    audio.extend(vec![0.0; 48_000]);
    audio.extend(tone(880.0, 48_000, 3.0));
    let got = texts(wav(48_000, 1, &audio), "audio/wav", &Options::default());
    assert_eq!(
        got,
        [
            (0, 3000, "tone 440 hertz".to_owned()),
            (4000, 7000, "tone 880 hertz".to_owned()),
        ]
    );
}

#[test]
fn long_recordings_are_windowed_and_capped() {
    // 50 s of alternating 10 s tones in 20 s windows (cut at quiet points), only
    // the first 45 s transcribed.
    let mut audio = Vec::new();
    for i in 0..5 {
        let freq = if i % 2 == 0 { 300.0 } else { 600.0 };
        audio.extend(tone(freq, 16_000, 9.5));
        audio.extend(vec![0.0; 8000]);
    }
    let options = Options {
        max_duration: Duration::from_secs(45),
        window: Duration::from_secs(20),
    };
    let mut reader = AudioReader::open(
        Box::new(std::io::Cursor::new(wav(16_000, 1, &audio))),
        "audio/wav",
    )
    .expect("open");
    let mut progress = Vec::new();
    let t = transcribe(
        &mut reader,
        &ToneTranscriber,
        &options,
        &Control::default(),
        &mut |p| progress.push(p),
    )
    .expect("transcribe");
    assert!(t.truncated);
    assert_eq!(t.duration_ms, 45_000);
    // Window cuts land in the quiet half seconds, so each tone is one segment.
    let got: Vec<(u32, &str)> = t
        .segments
        .iter()
        .map(|s| (s.start_ms, s.text.as_str()))
        .collect();
    let expected = [
        (0, "tone 300 hertz"),
        (10_000, "tone 600 hertz"),
        (20_000, "tone 300 hertz"),
        (30_000, "tone 600 hertz"),
        (40_000, "tone 300 hertz"),
    ];
    assert_eq!(got.len(), expected.len(), "{:?}", t.segments);
    for ((start, text), (want_start, want_text)) in got.iter().zip(expected) {
        assert_eq!(*text, want_text, "{:?}", t.segments);
        assert!(start.abs_diff(want_start) <= 1000, "{:?}", t.segments);
    }
    assert!(t.segments.iter().all(|s| s.end_ms <= 45_000));
    assert!(progress.len() >= 2 && progress.windows(2).all(|w| w[0] <= w[1]));
}

#[test]
fn cancelling_stops_the_transcription() {
    let control = Control::default();
    control.cancel();
    let mut reader = AudioReader::open(
        Box::new(std::io::Cursor::new(wav(
            16_000,
            1,
            &tone(440.0, 16_000, 2.0),
        ))),
        "audio/wav",
    )
    .expect("open");
    let err = transcribe(
        &mut reader,
        &ToneTranscriber,
        &Options::default(),
        &control,
        &mut |_| {},
    )
    .expect_err("cancelled");
    assert!(matches!(err, MediaError::Cancelled));
}

#[test]
fn real_formats_decode_and_ignore_video_tracks() {
    let cases = [
        ("tones.mp3", "audio/mpeg"),
        ("tones.opus", "audio/ogg"),
        ("tones.m4a", "audio/mp4"),
        ("tones.ogg", "audio/ogg"),
        ("tones.mp4", "video/mp4"),
        ("tones.webm", "video/webm"),
    ];
    for (file, mime) in cases {
        let path = format!("{}/tests/fixtures/{file}", env!("CARGO_MANIFEST_DIR"));
        let bytes = std::fs::read(&path).expect("fixture");
        let got = texts(bytes, mime, &Options::default());
        // Lossy codecs, priming delay and padding blur the pitch and the boundary a little.
        let hertz: Vec<f64> = got
            .iter()
            .map(|(_, _, t)| {
                t.trim_start_matches("tone ")
                    .trim_end_matches(" hertz")
                    .parse()
                    .expect("number")
            })
            .collect();
        assert_eq!(hertz.len(), 2, "{file}: {got:?}");
        assert!((hertz[0] - 440.0).abs() <= 20.0, "{file}: {got:?}");
        assert!((hertz[1] - 880.0).abs() <= 20.0, "{file}: {got:?}");
        assert_eq!(got[0].0, 0, "{file}");
        assert_eq!(got[1].0, 1000, "{file}");
    }
}

#[test]
fn garbage_and_silent_files_are_handled() {
    let err = AudioReader::open(
        Box::new(std::io::Cursor::new(b"not audio".to_vec())),
        "audio/mpeg",
    )
    .err()
    .expect("garbage");
    assert!(err.is_permanent(), "{err}");
    // Silence: nothing to say, but not an error.
    let silent = wav(16_000, 1, &vec![0.0; 32_000]);
    assert!(texts(silent, "audio/wav", &Options::default()).is_empty());
}
