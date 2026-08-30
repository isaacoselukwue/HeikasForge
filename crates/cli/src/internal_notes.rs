use std::path::Path;

use heikas_application::error::{ApplicationError, ApplicationResult};

pub const FILE_NAME: &str = "README.internal.md";

const HEADING: &str = "# Heikas Forge working notes";

const PREAMBLE: &str =
    "This file is deliberately local. It is neither tracked nor ignored, so it stays visible in `git status` without ever being published. Record whatever is useful to you here.";

const SECTIONS: [&str; 3] = [
    "Environment notes",
    "Release reminders",
    "Open observations",
];

pub fn scaffold() -> String {
    let mut document = String::from(HEADING);
    document.push_str("\n\n");
    document.push_str(PREAMBLE);
    document.push_str("\n\n");
    for section in SECTIONS {
        document.push_str(&format!("## {section}\n\n\n"));
    }
    document
}

pub fn write(repository: &Path) -> ApplicationResult<std::path::PathBuf> {
    let path = repository.join(FILE_NAME);
    if path.exists() {
        return Ok(path);
    }
    std::fs::write(&path, scaffold()).map_err(|error| {
        ApplicationError::Storage(format!("could not write `{}`: {error}", path.display()))
    })?;
    Ok(path)
}

pub fn refresh(repository: &Path) -> ApplicationResult<std::path::PathBuf> {
    let path = repository.join(FILE_NAME);
    let existing = std::fs::read_to_string(&path).unwrap_or_default();
    if existing.trim().is_empty() {
        std::fs::write(&path, scaffold()).map_err(|error| {
            ApplicationError::Storage(format!("could not write `{}`: {error}", path.display()))
        })?;
    }
    Ok(path)
}
