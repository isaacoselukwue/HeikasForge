use crate::error::PolicyResult;
use crate::finding::PolicyFinding;
use crate::repository::TrackedRepository;
use crate::rules::is_first_party_source;

pub const RULE: &str = "naming.no-vague-modules";

pub const PROHIBITED_NAMES: [&str; 8] = [
    "utils", "util", "helpers", "helper", "manager", "managers", "misc", "common",
];

pub fn check(repository: &TrackedRepository) -> PolicyResult<Vec<PolicyFinding>> {
    let mut findings = Vec::new();
    for path in &repository.tracked_files {
        if !is_first_party_source(path) {
            continue;
        }
        for segment in path.split('/') {
            let stem = segment
                .rsplit_once('.')
                .map(|(stem, _)| stem)
                .unwrap_or(segment)
                .to_ascii_lowercase();
            if PROHIBITED_NAMES.contains(&stem.as_str()) {
                findings.push(
                    PolicyFinding::violation(
                        RULE,
                        format!(
                            "the path segment `{segment}` is a prohibited vague container name"
                        ),
                        "Rename the module after the responsibility that it owns.",
                    )
                    .in_file(path.clone()),
                );
            }
        }
    }
    Ok(findings)
}
