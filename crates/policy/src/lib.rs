pub mod error;
pub mod finding;
pub mod lexer;
pub mod media;
pub mod repository;
pub mod rules;

pub use error::{PolicyError, PolicyResult};
pub use finding::{FindingSeverity, PolicyFinding, PolicyReport};
pub use repository::TrackedRepository;

use std::path::Path;

pub fn check_repository(root: &Path) -> PolicyResult<PolicyReport> {
    let repository = TrackedRepository::discover(root)?;
    rules::run_all(&repository)
}
