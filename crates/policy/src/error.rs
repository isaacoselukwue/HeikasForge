use thiserror::Error;

#[derive(Debug, Error)]
pub enum PolicyError {
    #[error("the repository at `{path}` could not be inspected: {detail}")]
    RepositoryUnreadable { path: String, detail: String },

    #[error("`git {arguments}` failed: {detail}")]
    GitFailed { arguments: String, detail: String },

    #[error("`{path}` could not be read: {detail}")]
    FileUnreadable { path: String, detail: String },

    #[error("the policy dictionary could not be loaded: {0}")]
    DictionaryInvalid(String),
}

pub type PolicyResult<T> = Result<T, PolicyError>;
