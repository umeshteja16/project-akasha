//! Audio and video transcription (ADR 0017).
//!
//! - [`AudioReader`] decodes the first audio track of a recording (pure Rust:
//!   Symphonia and `opus-decoder`) to 16 kHz mono, streamed packet by packet.
//! - [`transcribe`] feeds it to a [`Transcriber`] in windows and returns
//!   timestamped [`Segment`]s.
//! - [`WhisperTranscriber`] (feature `whisper`) runs whisper.cpp; models come from
//!   [`models`] (pinned SHA-256, downloaded on first use). [`ToneTranscriber`]
//!   is a deterministic fake for tests.
//!
//! Everything here is CPU-bound and blocking: call it from a blocking thread.

mod decode;
mod error;
mod fake;
pub mod models;
mod resample;
mod transcribe;
#[cfg(feature = "whisper")]
mod whisper;

pub use decode::{AudioReader, SAMPLE_RATE, decode_all};
pub use error::MediaError;
pub use fake::{FAKE_MODEL, ToneTranscriber};
pub use resample::Resampler;
pub use transcribe::{Control, Options, Segment, Transcriber, Transcript, transcribe};
#[cfg(feature = "whisper")]
pub use whisper::WhisperTranscriber;

/// Whether this build can run real Whisper models.
pub const WHISPER_AVAILABLE: bool = cfg!(feature = "whisper");

/// Run `f`, turning a panic in a decoder or the model bindings into an error:
/// one bad file must not take the worker down.
pub fn guard<T>(f: impl FnOnce() -> Result<T, MediaError>) -> Result<T, MediaError> {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)) {
        Ok(result) => result,
        Err(panic) => {
            let msg = panic
                .downcast_ref::<&str>()
                .map(|s| (*s).to_owned())
                .or_else(|| panic.downcast_ref::<String>().cloned())
                .unwrap_or_default();
            tracing::warn!(panic = %msg, "audio decoder panicked");
            Err(MediaError::Unsupported(
                "the audio could not be decoded".into(),
            ))
        }
    }
}
