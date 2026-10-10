//! Media errors, split by whether retrying could help.

/// Why a file could not be decoded or transcribed. Messages are safe to show to
/// the file's owner.
#[derive(Debug, thiserror::Error)]
pub enum MediaError {
    /// The file is damaged, has no audio, or uses a codec we cannot decode.
    /// Retrying cannot help.
    #[error("{0}")]
    Unsupported(String),
    /// Reading the file failed (disk). May succeed later.
    #[error("reading the audio failed: {0}")]
    Io(String),
    /// The speech model could not be fetched or loaded (network, disk). May succeed later.
    #[error("the speech recognition model is unavailable: {0}")]
    Models(String),
    /// The speech model failed on audio it could decode.
    #[error("transcription failed: {0}")]
    Transcribe(String),
    /// Stopped on request (worker shutdown).
    #[error("transcription was cancelled")]
    Cancelled,
}

impl MediaError {
    /// Retrying the same input cannot succeed.
    pub fn is_permanent(&self) -> bool {
        matches!(self, Self::Unsupported(_))
    }
}
