pub mod authorship;
pub mod dependencies;
pub mod documentation;
pub mod internal_readme;
pub mod leakage;
pub mod naming;
pub mod source;
pub mod spelling;
pub mod typography;

use crate::error::PolicyResult;
use crate::finding::PolicyReport;
use crate::repository::TrackedRepository;

pub const FIRST_PARTY_ROOTS: [&str; 5] = [
    "crates/",
    "apps/web/src/",
    "apps/web/tests/",
    "xtask/",
    "scripts/",
];

pub const GENERATED_PATHS: [&str; 4] = [
    "apps/web/src/generated/",
    "crates/api/assets/",
    "docs/media/",
    "fixtures/",
];

pub fn is_first_party_source(path: &str) -> bool {
    if GENERATED_PATHS.iter().any(|root| path.starts_with(root)) {
        return false;
    }
    FIRST_PARTY_ROOTS.iter().any(|root| path.starts_with(root))
}

pub fn is_tracked_text(path: &str) -> bool {
    const BINARY_EXTENSIONS: [&str; 12] = [
        "png", "jpg", "jpeg", "gif", "webm", "mp4", "ico", "woff", "woff2", "ttf", "zip", "pdf",
    ];
    match path.rsplit('.').next() {
        Some(extension) => !BINARY_EXTENSIONS.contains(&extension),
        None => true,
    }
}

pub fn run_all(repository: &TrackedRepository) -> PolicyResult<PolicyReport> {
    let mut report = PolicyReport {
        files_checked: repository.tracked_files.len() as u64,
        rules_run: vec![
            typography::RULE.to_string(),
            source::COMMENT_RULE.to_string(),
            source::MARKER_RULE.to_string(),
            naming::RULE.to_string(),
            spelling::RULE.to_string(),
            dependencies::RULE.to_string(),
            documentation::REMOTE_ASSET_RULE.to_string(),
            documentation::MEDIA_RULE.to_string(),
            leakage::HOST_PATH_RULE.to_string(),
            leakage::SECRET_RULE.to_string(),
            internal_readme::RULE.to_string(),
            authorship::RULE.to_string(),
        ],
        ..PolicyReport::default()
    };
    report.extend(typography::check(repository)?);
    report.extend(source::check(repository)?);
    report.extend(naming::check(repository)?);
    report.extend(spelling::check(repository)?);
    report.extend(dependencies::check(repository)?);
    report.extend(documentation::check(repository)?);
    report.extend(leakage::check(repository)?);
    report.extend(internal_readme::check(repository)?);
    report.extend(authorship::check(repository)?);
    Ok(report)
}
