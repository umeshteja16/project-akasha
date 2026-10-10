//! Real Whisper model (ignored by default; needs the model file, which is not
//! downloadable in every environment):
//!
//! ```sh
//! AKASHA_TEST_MODELS_DIR=./models AKASHA_TEST_WHISPER_MODEL=base \
//! AKASHA_TEST_SPEECH=/path/to/speech.wav AKASHA_TEST_SPEECH_WORD=hello \
//!   cargo test -p akasha-media --release --test whisper -- --ignored
//! ```
//!
//! Downloads the model from Hugging Face into the models directory when missing.
#![cfg(feature = "whisper")]

use std::path::PathBuf;

use akasha_media::{AudioReader, Control, Options, WhisperTranscriber, models, transcribe};

#[test]
#[ignore = "downloads a Whisper model; run with --ignored"]
fn whisper_transcribes_speech() {
    let dir = PathBuf::from(std::env::var("AKASHA_TEST_MODELS_DIR").unwrap_or("models".into()));
    let name = std::env::var("AKASHA_TEST_WHISPER_MODEL").unwrap_or("tiny".into());
    let models::ModelChoice::Whisper(model) = models::find(&name).expect("model") else {
        panic!("{name} is not a Whisper model");
    };
    let path = models::ensure(&dir, "https://huggingface.co", model).expect("model file");
    let whisper = WhisperTranscriber::load(&path, model.name, 4, None).expect("load");

    let (bytes, mime) = match std::env::var("AKASHA_TEST_SPEECH") {
        Ok(p) => (std::fs::read(&p).expect("speech file"), mime_of(&p)),
        // No speech at hand: a tone must at least run through without errors.
        Err(_) => (tone_wav(), "audio/wav"),
    };
    let mut reader = AudioReader::open(Box::new(std::io::Cursor::new(bytes)), mime).expect("open");
    let t = transcribe(
        &mut reader,
        &whisper,
        &Options::default(),
        &Control::default(),
        &mut |_| {},
    )
    .expect("transcribe");
    eprintln!("{:#?}", t.segments);
    if let Ok(word) = std::env::var("AKASHA_TEST_SPEECH_WORD") {
        let all = t
            .segments
            .iter()
            .map(|s| s.text.to_lowercase())
            .collect::<Vec<_>>()
            .join(" ");
        assert!(all.contains(&word.to_lowercase()), "{all}");
        assert!(
            t.segments
                .windows(2)
                .all(|w| w[0].start_ms <= w[1].start_ms)
        );
    }
}

fn mime_of(path: &str) -> &'static str {
    match path.rsplit('.').next() {
        Some("mp3") => "audio/mpeg",
        Some("m4a") => "audio/mp4",
        Some("ogg" | "opus") => "audio/ogg",
        Some("flac") => "audio/flac",
        Some("mp4") => "video/mp4",
        Some("webm") => "video/webm",
        _ => "audio/wav",
    }
}

fn tone_wav() -> Vec<u8> {
    let samples: Vec<i16> = (0..32_000)
        .map(|i| ((i as f32 * 440.0 * std::f32::consts::TAU / 16_000.0).sin() * 8000.0) as i16)
        .collect();
    let mut out = Vec::new();
    let data = (samples.len() * 2) as u32;
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&16_000u32.to_le_bytes());
    out.extend_from_slice(&32_000u32.to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data.to_le_bytes());
    for s in samples {
        out.extend_from_slice(&s.to_le_bytes());
    }
    out
}
