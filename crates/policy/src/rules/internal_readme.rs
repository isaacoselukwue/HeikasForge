use crate::error::PolicyResult;
use crate::finding::PolicyFinding;
use crate::repository::{path_is_ignored, path_is_tracked, TrackedRepository};

pub const RULE: &str = "internal-readme.untracked";
pub const PRIVATE_DOCUMENT_RULE: &str = "private-documents.untracked";
pub const INTERNAL_README: &str = "README.internal.md";

pub const PRIVATE_DOCUMENTS: [(&str, &str); 2] = [
    (
        "spec.md",
        "the product and engineering contract is a private working document",
    ),
    (
        "CLAUDE.md",
        "the implementation instructions are a private working document",
    ),
];

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
    for (path, reason) in PRIVATE_DOCUMENTS {
        if path_is_tracked(&repository.root, path) {
            findings.push(
                PolicyFinding::violation(
                    PRIVATE_DOCUMENT_RULE,
                    format!("`{path}` is tracked by Git, but {reason}"),
                    format!("Run `git rm --cached {path}` and keep it in `.git/info/exclude`."),
                )
                .in_file(path.to_string()),
            );
        }
        if repository
            .tracked_files
            .iter()
            .any(|tracked| tracked == path || tracked.ends_with(&format!("/{path}")))
        {
            findings.push(
                PolicyFinding::violation(
                    PRIVATE_DOCUMENT_RULE,
                    format!("a tracked file is named `{path}`, but {reason}"),
                    format!("Stop tracking every copy of `{path}`."),
                )
                .in_file(path.to_string()),
            );
        }
    }
    Ok(findings)
}
