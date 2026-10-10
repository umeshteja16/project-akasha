//! Speech to text with whisper.cpp (through `whisper-rs`).

use std::path::Path;

use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

use crate::{
    MediaError,
    transcribe::{Control, Segment, Transcriber},
};

/// Segments the model itself thinks are probably not speech are dropped.
const MAX_NO_SPEECH: f32 = 0.8;

/// A loaded Whisper model. Thread-safe: each call gets its own decoder state.
pub struct WhisperTranscriber {
    context: WhisperContext,
    name: String,
    threads: i32,
    /// ISO 639-1 code; `None` detects the language per window.
    language: Option<String>,
}

impl WhisperTranscriber {
    /// Load a GGML model file. `threads` = CPU threads per transcription.
    pub fn load(
        path: &Path,
        name: &str,
        threads: usize,
        language: Option<&str>,
    ) -> Result<Self, MediaError> {
        install_logging();
        let params = WhisperContextParameters {
            use_gpu: false,
            ..Default::default()
        };
        let context = WhisperContext::new_with_params(path, params)
            .map_err(|e| MediaError::Models(format!("loading {}: {e}", path.display())))?;
        Ok(Self {
            context,
            name: name.to_owned(),
            threads: i32::try_from(threads.max(1)).unwrap_or(4),
            language: language
                .map(str::trim)
                .filter(|l| !l.is_empty() && *l != "auto")
                .map(str::to_owned),
        })
    }
}

impl Transcriber for WhisperTranscriber {
    fn transcribe(&self, samples: &[f32], control: &Control) -> Result<Vec<Segment>, MediaError> {
        let err = |e: whisper_rs::WhisperError| MediaError::Transcribe(e.to_string());
        let mut state = self.context.create_state().map_err(err)?;
        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        params.set_n_threads(self.threads);
        params.set_language(Some(self.language.as_deref().unwrap_or("auto")));
        params.set_translate(false);
        params.set_print_special(false);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);
        params.set_suppress_blank(true);
        let cancelled = control.flag();
        params
            .set_abort_callback_safe(move || cancelled.load(std::sync::atomic::Ordering::Relaxed));
        let result = state.full(params, samples);
        if control.is_cancelled() {
            return Err(MediaError::Cancelled);
        }
        result.map_err(err)?;
        let mut segments = Vec::new();
        for segment in state.as_iter() {
            if segment.no_speech_probability() > MAX_NO_SPEECH {
                continue;
            }
            let text = segment.to_str_lossy().map_err(err)?;
            // Timestamps are in centiseconds.
            let at = |t: i64| u32::try_from(t.max(0).saturating_mul(10)).unwrap_or(u32::MAX);
            segments.push(Segment {
                start_ms: at(segment.start_timestamp()),
                end_ms: at(segment.end_timestamp()),
                text: text.trim().to_owned(),
            });
        }
        Ok(segments)
    }

    fn name(&self) -> &str {
        &self.name
    }
}

/// Send whisper.cpp's log lines to `tracing` instead of stderr (once).
fn install_logging() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(whisper_rs::install_logging_hooks);
}
