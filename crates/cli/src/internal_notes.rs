use std::path::Path;

use heikas_application::error::{ApplicationError, ApplicationResult};

pub const FILE_NAME: &str = "README.internal.md";

pub const TEMPLATE: &str = r#"# Heikas Forge internal working notes

This file is intentionally local, untracked and not ignored. Never stage, commit, export or release it.

## Owner

Isaac Oselukwue

## Local implementation checklist

- Confirm the target machine's Rust, Node.js, pnpm, Git and browser tooling.
- Preserve the repository-local Git author and committer name as Isaac Oselukwue.
- Keep hosted agents and hosted quality services optional.
- Validate the local model tool-calling path before real repository use.
- Exercise crash recovery at every persistence boundary.
- Run the deterministic three-candidate fixture before capturing public media.
- Capture the real dashboard, plan approval, run detail and candidate comparison screens.
- Generate WebM, MP4 and GIF derivatives from the real application.
- Run the complete policy check for British English, em dashes and source comments.
- Confirm this file remains visible as untracked and is not ignored.

## Private release reminders

- Review all public screenshots for repository paths, usernames, credentials and personal data.
- Inspect the redacted export manually once before the first public release.
- Verify no fixture claims to represent a real model run.
- Verify all introduced commits show only Isaac Oselukwue as author and committer.
- Check that public limitations remain honest after each release.

## Local observations

Record machine-specific paths, temporary debugging findings and release notes below. Do not copy secrets into this file.
"#;

pub fn write(repository: &Path) -> ApplicationResult<std::path::PathBuf> {
    let path = repository.join(FILE_NAME);
    if path.exists() {
        return Ok(path);
    }
    std::fs::write(&path, TEMPLATE).map_err(|error| {
        ApplicationError::Storage(format!("could not write `{}`: {error}", path.display()))
    })?;
    Ok(path)
}

pub fn refresh(repository: &Path) -> ApplicationResult<std::path::PathBuf> {
    let path = repository.join(FILE_NAME);
    let existing = std::fs::read_to_string(&path).unwrap_or_default();
    if existing.trim().is_empty() {
        std::fs::write(&path, TEMPLATE).map_err(|error| {
            ApplicationError::Storage(format!("could not write `{}`: {error}", path.display()))
        })?;
    }
    Ok(path)
}
