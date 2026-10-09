//! Extraction errors, split by whether retrying could help.

/// Why a file could not be extracted. Messages are safe to show to the file's owner.
#[derive(Debug, thiserror::Error)]
pub enum IngestError {
    /// The file is damaged or not what its type claims. Retrying cannot help.
    #[error("{0}")]
    Corrupt(String),
    /// The PDF needs a password.
    #[error("this PDF is password-protected, so its text cannot be extracted")]
    Encrypted,
    /// The OCR models could not be fetched or loaded (network, disk). May succeed later.
    #[error("OCR models are unavailable: {0}")]
    Models(String),
    /// OCR failed on an image it could decode.
    #[error("text recognition failed: {0}")]
    Ocr(String),
}

impl IngestError {
    /// Retrying the same input cannot succeed.
    pub fn is_permanent(&self) -> bool {
        matches!(self, Self::Corrupt(_) | Self::Encrypted)
    }

    pub(crate) fn corrupt(what: &str, detail: impl std::fmt::Display) -> Self {
        tracing::debug!(%detail, "{what}");
        Self::Corrupt(what.to_owned())
    }
}

/// Run `f`, turning a panic into `Err(on_panic)`. Parser crates (pdf-extract,
/// image decoders) panic on some malformed inputs; one bad file must not take the
/// worker down.
pub(crate) fn guard<T>(
    on_panic: impl FnOnce() -> IngestError,
    f: impl FnOnce() -> Result<T, IngestError>,
) -> Result<T, IngestError> {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)) {
        Ok(result) => result,
        Err(panic) => {
            let msg = panic
                .downcast_ref::<&str>()
                .map(|s| (*s).to_owned())
                .or_else(|| panic.downcast_ref::<String>().cloned())
                .unwrap_or_default();
            tracing::warn!(panic = %msg, "parser panicked");
            Err(on_panic())
        }
    }
}
