use crate::error::PolicyResult;
use crate::finding::PolicyFinding;
use crate::repository::{path_is_ignored, path_is_tracked, TrackedRepository};

pub const RULE: &str = "internal-readme.untracked";
pub const INTERNAL_README: &str = "README.internal.md";

pub fn check(repository: &TrackedRepository) -> PolicyResult<Vec<PolicyFinding>> {
    let mut findings = Vec::new();
    if path_is_tracked(&repository.root, INTERNAL_README) {
        findings.push(
            PolicyFinding::violation(
                RULE,
                "the internal working notes file is tracked by Git",
                "Run `git rm --cached README.internal.md` and never stage it again.",
            )
            .in_file(INTERNAL_README.to_string()),
        );
    }
    if path_is_ignored(&repository.root, INTERNAL_README) {
        findings.push(
            PolicyFinding::violation(
                RULE,
                "the internal working notes file is listed in an ignore rule",
                "Remove README.internal.md from .gitignore and .git/info/exclude.",
            )
            .in_file(INTERNAL_README.to_string()),
        );
    }
    for rule in repository.ignore_rules()? {
        if rule.contains(INTERNAL_README) {
            findings.push(PolicyFinding::violation(
                RULE,
                format!("an ignore rule mentions the internal working notes file: `{rule}`"),
                "Delete the ignore rule so the file stays deliberately visible and untracked.",
            ));
        }
    }
    Ok(findings)
}
