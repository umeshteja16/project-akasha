//! Transcribing audio and video inside `extract_file`.
//!
//! The blob is streamed to a temporary file (recordings can be hundreds of MB),
//! decoded and transcribed on a blocking thread while this task reports progress
//! (`jobs.progress`) every few seconds. If the job is dropped (worker shutdown
//! aborts it), the transcription is told to stop.

use std::{sync::Arc, time::Duration};

use akasha_ingest::{ChunkOptions, Extraction, TimedText};
use akasha_media::{AudioReader, Control, MediaError, Transcriber};
use akasha_storage::ContentHash;
use futures_util::StreamExt;
use tokio::io::AsyncWriteExt;

use super::JobContext;

/// Transcript chunks are smaller than text chunks (~1 minute of speech), so a
/// search hit or citation points close to where something was said.
pub const TRANSCRIPT_CHUNKS: ChunkOptions = ChunkOptions {
    max_chars: 1000,
    overlap_chars: 150,
};

/// How often progress is written while transcribing.
const PROGRESS_EVERY: Duration = Duration::from_secs(3);

/// A transcript as an [`Extraction`] plus the length of audio transcribed.
pub struct Transcribed {
    pub extraction: Extraction,
    pub duration_ms: u64,
}

/// Cancels the transcription when the job's future is dropped.
struct CancelOnDrop(Control);

impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        self.0.cancel();
    }
}

/// Transcribe the recording stored under `hash` with `model`.
pub async fn transcribe(
    ctx: &JobContext,
    model: Arc<dyn Transcriber>,
    hash: &ContentHash,
    mime: &str,
) -> Result<Transcribed, MediaError> {
    let file = download(ctx, hash).await?;
    let _turn = ctx.transcriber.turn().await;
    let started = std::time::Instant::now();
    let control = Control::default();
    let _cancel = CancelOnDrop(control.clone());
    let options = ctx.transcriber.options();
    let max_minutes = ctx.transcriber.max_minutes();
    let (tx, mut rx) = tokio::sync::watch::channel(0.0f32);
    let mime = mime.to_owned();
    let worker_control = control.clone();
    let mut task = tokio::task::spawn_blocking(move || {
        akasha_media::guard(|| {
            let source = file.reopen().map_err(|e| MediaError::Io(e.to_string()))?;
            let mut reader = AudioReader::open(Box::new(source), &mime)?;
            akasha_media::transcribe(
                &mut reader,
                model.as_ref(),
                &options,
                &worker_control,
                &mut |p| {
                    let _ = tx.send(p);
                },
            )
        })
    });
    let mut tick = tokio::time::interval(PROGRESS_EVERY);
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut reported = -1.0f32;
    let transcript = loop {
        tokio::select! {
            joined = &mut task => {
                break joined.map_err(|e| MediaError::Transcribe(format!("task failed: {e}")))??;
            }
            _ = tick.tick() => {
                let now = *rx.borrow_and_update();
                if now > reported {
                    reported = now;
                    akasha_jobs::report_progress(&ctx.db, now).await;
                }
            }
        }
    };
    crate::metrics::ingest("transcribe", started);

    let segments: Vec<TimedText<'_>> = transcript
        .segments
        .iter()
        .map(|s| TimedText {
            start_ms: s.start_ms,
            end_ms: s.end_ms,
            text: &s.text,
        })
        .collect();
    let mut notes = vec![format!(
        "transcribed automatically ({}); expect some mistakes",
        model_label(ctx.transcriber.model_name())
    )];
    if transcript.truncated {
        notes.push(format!(
            "only the first {max_minutes} minutes were transcribed"
        ));
    }
    let extraction = akasha_ingest::transcript(&segments, akasha_ingest::DEFAULT_MAX_CHARS, notes);
    Ok(Transcribed {
        extraction,
        duration_ms: transcript.duration_ms,
    })
}

fn model_label(name: &str) -> String {
    if name == akasha_media::FAKE_MODEL {
        "test model".into()
    } else {
        format!("Whisper {name}")
    }
}

/// Copy the blob into a temporary file (deleted when dropped).
async fn download(
    ctx: &JobContext,
    hash: &ContentHash,
) -> Result<tempfile::NamedTempFile, MediaError> {
    let io = |e: std::io::Error| MediaError::Io(e.to_string());
    let blob = match ctx.storage.get(hash).await {
        Ok(blob) => blob,
        Err(akasha_storage::StorageError::NotFound) => {
            return Err(MediaError::Unsupported(
                "the file's contents are missing".into(),
            ));
        }
        Err(e) => return Err(MediaError::Io(e.to_string())),
    };
    let tmp = tempfile::NamedTempFile::new().map_err(io)?;
    let mut out = tokio::fs::File::from_std(tmp.reopen().map_err(io)?);
    let mut stream = blob.stream;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| MediaError::Io(e.to_string()))?;
        out.write_all(&chunk).await.map_err(io)?;
    }
    out.flush().await.map_err(io)?;
    Ok(tmp)
}
