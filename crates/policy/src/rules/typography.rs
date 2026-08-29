use crate::error::PolicyResult;
use crate::finding::PolicyFinding;
use crate::repository::TrackedRepository;
use crate::rules::is_tracked_text;

pub const RULE: &str = "typography.em-dash";

pub fn forbidden_character() -> char {
    char::from_u32(0x2014).unwrap_or('-')
}

pub fn check(repository: &TrackedRepository) -> PolicyResult<Vec<PolicyFinding>> {
    let forbidden = forbidden_character();
    let mut findings = Vec::new();
    for path in &repository.tracked_files {
        if !is_tracked_text(path) {
            continue;
        }
        let Some(contents) = repository.read_text(path)? else {
            continue;
        };
        for (index, line) in contents.lines().enumerate() {
            if let Some(column) = line.find(forbidden) {
                findings.push(
                    PolicyFinding::violation(
                        RULE,
                        "a tracked text file contains the forbidden em dash character",
                        "Replace it with a full stop, colon, comma or ordinary hyphen.",
                    )
                    .at(path.clone(), index as u32 + 1, column as u32 + 1),
                );
            }
        }
    }
    Ok(findings)
}
