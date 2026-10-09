use thiserror::Error;

/// Why a model could not be loaded or run.
#[derive(Debug, Error)]
pub enum MlError {
    /// The configured model name is not in the catalog (a configuration mistake).
    #[error("unknown {kind} model `{name}`; supported: {supported}")]
    UnknownModel {
        kind: &'static str,
        name: String,
        supported: String,
    },
    /// Model files are missing and could not be downloaded.
    #[error("model files unavailable: {0}")]
    Models(String),
    /// ONNX Runtime could not be loaded (shared library missing or too old).
    #[error("ONNX Runtime unavailable: {0}")]
    Runtime(String),
    /// Loading or running the model failed.
    #[error("inference failed: {0}")]
    Inference(String),
}

impl MlError {
    /// `true` when retrying cannot help without a configuration change.
    pub fn is_permanent(&self) -> bool {
        matches!(self, Self::UnknownModel { .. })
    }
}
