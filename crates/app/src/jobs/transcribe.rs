//! The process-wide speech model, loaded (and its file downloaded) on first use.
//!
//! Only one recording is transcribed at a time per process ([`TranscriberProvider::turn`]):
//! a transcription already uses several cores, and running two would only make
//! both slower while starving the other jobs.

use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};

use akasha_core::Config;
use akasha_media::{MediaError, Options, ToneTranscriber, Transcriber, models};
use tokio::sync::{OnceCell, Semaphore, SemaphorePermit};

pub struct TranscriberProvider {
    enabled: bool,
    model: String,
    dir: PathBuf,
    endpoint: String,
    threads: usize,
    language: String,
    max_minutes: u32,
    engine: OnceCell<Arc<dyn Transcriber>>,
    error: Mutex<Option<String>>,
    turn: Semaphore,
}

impl TranscriberProvider {
    pub fn from_config(config: &Config) -> Self {
        let cores = std::thread::available_parallelism().map_or(4, |n| n.get());
        let threads = match config.transcribe_threads {
            0 => cores.min(8),
            n => usize::try_from(n).unwrap_or(cores),
        };
        Self {
            enabled: config.transcribe_enabled,
            model: config.whisper_model.trim().to_owned(),
            dir: PathBuf::from(&config.models_dir),
            endpoint: config.ml_models_url.trim().to_owned(),
            threads,
            language: config.transcribe_language.trim().to_owned(),
            max_minutes: config.transcribe_max_minutes.max(1),
            engine: OnceCell::new(),
            error: Mutex::new(None),
            turn: Semaphore::new(1),
        }
    }

    pub fn enabled(&self) -> bool {
        self.enabled
    }

    pub fn model_name(&self) -> &str {
        &self.model
    }

    /// Windowing and the duration cap.
    pub fn options(&self) -> Options {
        Options {
            max_duration: Duration::from_secs(u64::from(self.max_minutes) * 60),
            ..Options::default()
        }
    }

    pub fn max_minutes(&self) -> u32 {
        self.max_minutes
    }

    /// Wait until no other transcription runs in this process.
    pub async fn turn(&self) -> Option<SemaphorePermit<'_>> {
        self.turn.acquire().await.ok()
    }

    /// The loaded model; `None` when transcription is turned off.
    pub async fn get(&self) -> Result<Option<Arc<dyn Transcriber>>, MediaError> {
        if !self.enabled {
            return Ok(None);
        }
        let engine = self
            .engine
            .get_or_try_init(|| async {
                let (model, dir, endpoint) =
                    (self.model.clone(), self.dir.clone(), self.endpoint.clone());
                let (threads, language) = (self.threads, self.language.clone());
                tokio::task::spawn_blocking(move || {
                    load(&model, &dir, &endpoint, threads, &language)
                })
                .await
                .map_err(|e| MediaError::Models(format!("loading the model panicked: {e}")))?
            })
            .await;
        if let Ok(mut slot) = self.error.lock() {
            *slot = engine.as_ref().err().map(ToString::to_string);
        }
        Ok(Some(Arc::clone(engine?)))
    }

    /// Why the last load failed, if it did.
    pub fn last_error(&self) -> Option<String> {
        self.error.lock().ok().and_then(|e| e.clone())
    }

    pub fn is_loaded(&self) -> bool {
        self.engine.initialized()
    }
}

fn load(
    name: &str,
    dir: &std::path::Path,
    endpoint: &str,
    threads: usize,
    language: &str,
) -> Result<Arc<dyn Transcriber>, MediaError> {
    match models::find(name)? {
        models::ModelChoice::Fake => Ok(Arc::new(ToneTranscriber)),
        models::ModelChoice::Whisper(model) => whisper(model, dir, endpoint, threads, language),
    }
}

#[cfg(feature = "whisper")]
fn whisper(
    model: &models::WhisperModel,
    dir: &std::path::Path,
    endpoint: &str,
    threads: usize,
    language: &str,
) -> Result<Arc<dyn Transcriber>, MediaError> {
    let path = models::ensure(dir, endpoint, model)?;
    let language = (!language.is_empty()).then_some(language);
    let loaded = akasha_media::WhisperTranscriber::load(&path, model.name, threads, language)?;
    tracing::info!(model = model.name, threads, "speech model loaded");
    Ok(Arc::new(loaded))
}

#[cfg(not(feature = "whisper"))]
fn whisper(
    _model: &models::WhisperModel,
    _dir: &std::path::Path,
    _endpoint: &str,
    _threads: usize,
    _language: &str,
) -> Result<Arc<dyn Transcriber>, MediaError> {
    Err(MediaError::Models(
        "this build has no Whisper support (built without the `whisper` feature)".into(),
    ))
}
