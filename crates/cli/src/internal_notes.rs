use std::path::Path;

use heikas_application::error::ApplicationResult;

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
    if std::fs::symlink_metadata(&path).is_ok_and(|metadata| metadata.is_file()) {
        return Ok(path);
    }
    heikas_infrastructure::atomic::write_atomic(&path, scaffold().as_bytes())?;
    Ok(path)
}

pub fn refresh(repository: &Path) -> ApplicationResult<std::path::PathBuf> {
    let path = repository.join(FILE_NAME);
    let existing = match std::fs::symlink_metadata(&path) {
        Ok(metadata) if metadata.is_file() => std::fs::read_to_string(&path).unwrap_or_default(),
        _ => String::new(),
    };
    if existing.trim().is_empty() {
        heikas_infrastructure::atomic::write_atomic(&path, scaffold().as_bytes())?;
    }
    Ok(path)
}
