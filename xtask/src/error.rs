use thiserror::Error;

#[derive(Debug, Error)]
pub enum TaskError {
    #[error("{0}")]
    Process(String),

    #[error("{0}")]
    Filesystem(String),

    #[error("{0}")]
    Missing(String),

    #[error("{0}")]
    Invalid(String),

    #[error("the verification step `{step}` failed")]
    StepFailed { step: String },

    #[error("{0}")]
    Encoding(String),
}

pub type TaskResult<T> = Result<T, TaskError>;

impl From<serde_json::Error> for TaskError {
    fn from(error: serde_json::Error) -> Self {
        TaskError::Encoding(error.to_string())
    }
}

impl From<std::io::Error> for TaskError {
    fn from(error: std::io::Error) -> Self {
        TaskError::Filesystem(error.to_string())
    }
}
