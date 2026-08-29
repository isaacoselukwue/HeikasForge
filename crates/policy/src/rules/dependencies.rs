use crate::error::PolicyResult;
use crate::finding::PolicyFinding;
use crate::repository::TrackedRepository;

pub const RULE: &str = "dependencies.no-paid-service";

pub const PROHIBITED_RUNTIME_DEPENDENCIES: [&str; 12] = [
    "sentry",
    "datadog",
    "newrelic",
    "segment",
    "mixpanel",
    "amplitude",
    "launchdarkly",
    "auth0",
    "firebase",
    "supabase",
    "sonarcloud",
    "posthog",
];

pub const MANIFEST_PATHS: [&str; 3] = ["Cargo.toml", "apps/web/package.json", "package.json"];

pub fn check(repository: &TrackedRepository) -> PolicyResult<Vec<PolicyFinding>> {
    let mut findings = Vec::new();
    for path in MANIFEST_PATHS {
        let Some(contents) = repository.read_text(path)? else {
            continue;
        };
        let lowered = contents.to_ascii_lowercase();
        for name in PROHIBITED_RUNTIME_DEPENDENCIES {
            if lowered.contains(name) {
                findings.push(
                    PolicyFinding::violation(
                        RULE,
                        format!("`{path}` references the hosted service dependency `{name}`"),
                        "The mandatory runtime path must stay free of paid or hosted services.",
                    )
                    .in_file(path.to_string()),
                );
            }
        }
    }

    for path in &repository.tracked_files {
        if !path.starts_with("crates/") || !path.ends_with("Cargo.toml") {
            continue;
        }
        let Some(contents) = repository.read_text(path)? else {
            continue;
        };
        let lowered = contents.to_ascii_lowercase();
        for name in PROHIBITED_RUNTIME_DEPENDENCIES {
            if lowered.contains(name) {
                findings.push(
                    PolicyFinding::violation(
                        RULE,
                        format!("`{path}` references the hosted service dependency `{name}`"),
                        "Move the dependency behind an optional adapter or remove it.",
                    )
                    .in_file(path.clone()),
                );
            }
        }
    }
    Ok(findings)
}
