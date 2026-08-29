use serde::Deserialize;

use crate::error::{PolicyError, PolicyResult};
use crate::finding::PolicyFinding;
use crate::lexer::{scan, SourceLanguage};
use crate::repository::TrackedRepository;
use crate::rules::is_first_party_source;

pub const RULE: &str = "spelling.british-english";
pub const DICTIONARY_PATH: &str = "crates/policy/dictionary.toml";

const DICTIONARY_SOURCE: &str = include_str!("../../dictionary.toml");

#[derive(Debug, Clone, Deserialize)]
pub struct Dictionary {
    pub description: String,
    #[serde(default)]
    pub exempt_literals: Vec<String>,
    pub entries: Vec<DictionaryEntry>,
}

impl Dictionary {
    pub fn literal_is_exempt(&self, literal: &str) -> bool {
        let trimmed = literal.trim().to_ascii_lowercase();
        self.exempt_literals
            .iter()
            .any(|candidate| candidate.eq_ignore_ascii_case(&trimmed))
    }
}

pub fn looks_like_regular_expression(literal: &str) -> bool {
    const MARKERS: [&str; 12] = [
        "(?i)", "(?:", "\\b", "\\s", "\\d", "\\w", "[a-z", "[A-Z", "[0-9", "{2,", "+?", "]*",
    ];
    MARKERS.iter().any(|marker| literal.contains(marker))
}

#[derive(Debug, Clone, Deserialize)]
pub struct DictionaryEntry {
    pub discouraged: String,
    pub preferred: String,
}

pub fn dictionary() -> PolicyResult<Dictionary> {
    toml::from_str(DICTIONARY_SOURCE)
        .map_err(|error| PolicyError::DictionaryInvalid(error.to_string()))
}

pub fn check(repository: &TrackedRepository) -> PolicyResult<Vec<PolicyFinding>> {
    let dictionary = dictionary()?;
    let mut findings = Vec::new();
    for path in &repository.tracked_files {
        if path == DICTIONARY_PATH {
            continue;
        }
        let Some(contents) = repository.read_text(path)? else {
            continue;
        };
        if path.ends_with(".md") {
            findings.extend(check_markdown(path, &contents, &dictionary));
            continue;
        }
        if !is_first_party_source(path) {
            continue;
        }
        let Some(language) = SourceLanguage::for_path(path) else {
            continue;
        };
        if language == SourceLanguage::Css {
            continue;
        }
        let outcome = scan(language, &contents);
        for literal in &outcome.literals {
            if dictionary.literal_is_exempt(&literal.text)
                || looks_like_regular_expression(&literal.text)
            {
                continue;
            }
            for entry in &dictionary.entries {
                if contains_token(&literal.text, &entry.discouraged) {
                    findings.push(
                        PolicyFinding::violation(
                            RULE,
                            format!(
                                "user-facing text uses `{}` instead of `{}`",
                                entry.discouraged, entry.preferred
                            ),
                            format!("Replace `{}` with `{}`.", entry.discouraged, entry.preferred),
                        )
                        .at(path.clone(), literal.line, literal.column),
                    );
                }
            }
        }
    }
    Ok(findings)
}

fn check_markdown(path: &str, contents: &str, dictionary: &Dictionary) -> Vec<PolicyFinding> {
    let mut findings = Vec::new();
    let mut in_fence = false;
    for (index, line) in contents.lines().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            continue;
        }
        let prose = strip_inline_code_and_links(line);
        for entry in &dictionary.entries {
            if contains_token(&prose, &entry.discouraged) {
                findings.push(
                    PolicyFinding::violation(
                        RULE,
                        format!(
                            "documentation prose uses `{}` instead of `{}`",
                            entry.discouraged, entry.preferred
                        ),
                        format!("Replace `{}` with `{}`.", entry.discouraged, entry.preferred),
                    )
                    .at(path.to_string(), index as u32 + 1, 1),
                );
            }
        }
    }
    findings
}

fn strip_inline_code_and_links(line: &str) -> String {
    let mut without_code = String::new();
    let mut in_code = false;
    for character in line.chars() {
        match character {
            '`' => in_code = !in_code,
            _ if in_code => {}
            other => without_code.push(other),
        }
    }
    let mut prose = String::new();
    for token in without_code.split_whitespace() {
        if token.contains("://") || token.starts_with('/') || token.contains('@') {
            continue;
        }
        prose.push_str(token);
        prose.push(' ');
    }
    prose
}

pub fn contains_token(text: &str, token: &str) -> bool {
    let lowered = text.to_ascii_lowercase();
    lowered
        .split(|character: char| !is_token_character(character))
        .any(|candidate| candidate == token)
}

fn is_token_character(character: char) -> bool {
    character.is_alphanumeric() || character == '_' || character == '-'
}
