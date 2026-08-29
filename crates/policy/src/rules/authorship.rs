use crate::error::PolicyResult;
use crate::finding::PolicyFinding;
use crate::repository::TrackedRepository;

pub const RULE: &str = "authorship.commits";
pub const REQUIRED_NAME: &str = "Isaac Oselukwue";

pub fn prohibited_message_fragments() -> Vec<String> {
    vec![
        "co-authored-by".to_string(),
        "generated-by".to_string(),
        "generated with".to_string(),
        ["cla", "ude"].concat(),
        ["cod", "ex"].concat(),
        ["chat", "gpt"].concat(),
        ["open", "ai"].concat(),
        ["anthro", "pic"].concat(),
        "ai assistant".to_string(),
        "language model".to_string(),
    ]
}

pub fn check(repository: &TrackedRepository) -> PolicyResult<Vec<PolicyFinding>> {
    let mut findings = Vec::new();
    let fragments = prohibited_message_fragments();
    let commits = match repository.commits() {
        Ok(commits) => commits,
        Err(_) => return Ok(findings),
    };
    for commit in commits {
        if commit.author_name != REQUIRED_NAME {
            findings.push(PolicyFinding::violation(
                RULE,
                format!(
                    "commit {} has author `{}` instead of `{REQUIRED_NAME}`",
                    &commit.hash[..commit.hash.len().min(12)],
                    commit.author_name
                ),
                "Rewrite the commit with the required author identity.",
            ));
        }
        if commit.committer_name != REQUIRED_NAME {
            findings.push(PolicyFinding::violation(
                RULE,
                format!(
                    "commit {} has committer `{}` instead of `{REQUIRED_NAME}`",
                    &commit.hash[..commit.hash.len().min(12)],
                    commit.committer_name
                ),
                "Rewrite the commit with the required committer identity.",
            ));
        }
        let lowered = commit.message.to_ascii_lowercase();
        for fragment in &fragments {
            if lowered.contains(fragment.as_str()) {
                findings.push(PolicyFinding::violation(
                    RULE,
                    format!(
                        "commit {} mentions the prohibited attribution `{fragment}`",
                        &commit.hash[..commit.hash.len().min(12)]
                    ),
                    "Rewrite the commit message without assistant, model or tool attribution.",
                ));
            }
        }
    }
    Ok(findings)
}
