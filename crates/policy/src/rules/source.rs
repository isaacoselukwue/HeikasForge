use crate::error::PolicyResult;
use crate::finding::PolicyFinding;
use crate::lexer::{scan, SourceLanguage};
use crate::repository::TrackedRepository;
use crate::rules::is_first_party_source;

pub const COMMENT_RULE: &str = "source.no-comments";
pub const MARKER_RULE: &str = "source.no-task-markers";

pub fn task_markers() -> [String; 3] {
    [
        ["TO", "DO"].concat(),
        ["FIX", "ME"].concat(),
        ["HA", "CK"].concat(),
    ]
}

pub fn check(repository: &TrackedRepository) -> PolicyResult<Vec<PolicyFinding>> {
    let mut findings = Vec::new();
    let markers = task_markers();
    for path in &repository.tracked_files {
        if !is_first_party_source(path) {
            continue;
        }
        let Some(language) = SourceLanguage::for_path(path) else {
            continue;
        };
        let Some(contents) = repository.read_text(path)? else {
            continue;
        };
        let outcome = scan(language, &contents);
        for comment in &outcome.comments {
            let preview: String = comment.text.chars().take(60).collect();
            findings.push(
                PolicyFinding::violation(
                    COMMENT_RULE,
                    format!(
                        "first-party {} source contains a comment: {preview}",
                        language.as_str()
                    ),
                    "Remove the comment and let names, types and module boundaries carry the meaning.",
                )
                .at(path.clone(), comment.line, comment.column),
            );
        }
        for (index, line) in contents.lines().enumerate() {
            for marker in &markers {
                if let Some(column) = find_token(line, marker) {
                    findings.push(
                        PolicyFinding::violation(
                            MARKER_RULE,
                            format!("first-party source contains the task marker `{marker}`"),
                            "Complete the work or record it in the tracked architecture documentation.",
                        )
                        .at(path.clone(), index as u32 + 1, column as u32 + 1),
                    );
                }
            }
        }
    }
    Ok(findings)
}

fn find_token(line: &str, token: &str) -> Option<usize> {
    let mut search_from = 0usize;
    while let Some(relative) = line[search_from..].find(token) {
        let start = search_from + relative;
        let end = start + token.len();
        let before_ok = start == 0
            || !line[..start]
                .chars()
                .next_back()
                .map(is_token_character)
                .unwrap_or(false);
        let after_ok = end >= line.len()
            || !line[end..]
                .chars()
                .next()
                .map(is_token_character)
                .unwrap_or(false);
        if before_ok && after_ok {
            return Some(start);
        }
        search_from = end;
    }
    None
}

fn is_token_character(character: char) -> bool {
    character.is_alphanumeric() || character == '_' || character == '-'
}
