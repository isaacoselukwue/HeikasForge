use crate::error::PolicyResult;
use crate::finding::PolicyFinding;
use crate::repository::TrackedRepository;
use crate::rules::is_tracked_text;

pub const HOST_PATH_RULE: &str = "leakage.no-host-paths";
pub const SECRET_RULE: &str = "leakage.no-secret-material";

pub const RULE_SOURCE_PATH: &str = "crates/policy/src/rules/leakage.rs";
pub const RULE_TEST_PATH: &str = "crates/policy/tests/leakage_rules.rs";

pub const PLACEHOLDER_ACCOUNTS: [&str; 8] = [
    "you", "operator", "user", "username", "runner", "ci", "example", "someone",
];

const RUNTIME_DESCRIPTOR_KEYS: [&str; 3] = ["\"serverpid\"", "\"bootstrapurl\"", "\"heikashome\""];

pub fn check(repository: &TrackedRepository) -> PolicyResult<Vec<PolicyFinding>> {
    let mut findings = Vec::new();
    for path in &repository.tracked_files {
        if !is_tracked_text(path)
            || path == crate::rules::spelling::DICTIONARY_PATH
            || path == RULE_SOURCE_PATH
            || path == RULE_TEST_PATH
        {
            continue;
        }
        let Some(contents) = repository.read_text(path)? else {
            continue;
        };
        if let Some(line_number) = private_key_block(&contents) {
            findings.push(
                PolicyFinding::violation(
                    SECRET_RULE,
                    "a tracked file contains a private key block",
                    "Remove the key from version control, rotate it, and add the file to the ignore rules.",
                )
                .at(path.clone(), line_number, 1),
            );
        }
        for (index, line) in contents.lines().enumerate() {
            if let Some(account) = host_account(line) {
                findings.push(
                    PolicyFinding::violation(
                        HOST_PATH_RULE,
                        format!(
                            "a tracked file records the home directory of the account `{account}`"
                        ),
                        "Replace it with a documented placeholder, or stop tracking the file if it is a run artefact.",
                    )
                    .at(path.clone(), index as u32 + 1, 1),
                );
            }
            if let Some(shape) = secret_shape(line) {
                findings.push(
                    PolicyFinding::violation(
                        SECRET_RULE,
                        format!("a tracked file contains {shape}"),
                        "Remove the value from version control, rotate it, and add the file to the ignore rules if it is a run artefact.",
                    )
                    .at(path.clone(), index as u32 + 1, 1),
                );
            }
        }
    }
    Ok(findings)
}

pub fn host_account(line: &str) -> Option<String> {
    for (prefix, separator) in [("/home/", '/'), ("/Users/", '/'), ("C:\\Users\\", '\\')] {
        let mut search_from = 0usize;
        while let Some(relative) = line[search_from..].find(prefix) {
            let start = search_from + relative + prefix.len();
            let remainder = &line[start..];
            let account: String = remainder
                .chars()
                .take_while(|character| *character != separator && !character.is_whitespace())
                .collect();
            search_from = start.max(search_from + 1);
            if account.is_empty() || remainder.len() == account.len() {
                continue;
            }
            if !account.chars().all(|character| {
                character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-')
            }) {
                continue;
            }
            let lowered = account.to_ascii_lowercase();
            if PLACEHOLDER_ACCOUNTS.contains(&lowered.as_str()) {
                continue;
            }
            return Some(account);
        }
    }
    None
}

pub fn secret_shape(line: &str) -> Option<&'static str> {
    let lowered = line.to_ascii_lowercase();

    if let Some(index) = lowered.find("token=") {
        let value = collect_value(&line[index + "token=".len()..], |character| {
            character.is_ascii_hexdigit()
        });
        if value.len() >= 32 && looks_random(&value) {
            return Some("a session token");
        }
    }

    for prefix in ["ghp_", "gho_", "ghu_", "ghs_", "ghr_", "github_pat_"] {
        if let Some(value) = value_after(line, prefix, 20, token_character) {
            if looks_random(&value) {
                return Some("a source forge access token");
            }
        }
    }

    for prefix in ["sk-ant-", "sk-proj-", "sk-"] {
        if let Some(value) = value_after(line, prefix, 20, token_character) {
            if looks_random(&value) {
                return Some("a model provider api key");
            }
        }
    }

    for prefix in ["xoxb-", "xoxp-", "xoxa-", "xoxr-", "xoxs-"] {
        if let Some(value) = value_after(line, prefix, 12, token_character) {
            if looks_random(&value) {
                return Some("a messaging platform token");
            }
        }
    }

    if let Some(value) = value_after(line, "AKIA", 16, |character| {
        character.is_ascii_uppercase() || character.is_ascii_digit()
    }) {
        if value.len() == 16 && looks_random(&value) {
            return Some("a cloud access key identifier");
        }
    }

    if let Some(value) = value_after(line, "AIza", 35, token_character) {
        if looks_random(&value) {
            return Some("a cloud api key");
        }
    }

    if let Some(value) = value_after(line, "eyJ", 20, |character| {
        character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.')
    }) {
        if value.matches('.').count() >= 2 && looks_random(&value.replace('.', "")) {
            return Some("a signed web token");
        }
    }

    if RUNTIME_DESCRIPTOR_KEYS
        .iter()
        .any(|key| lowered.contains(key))
    {
        return Some("a captured runtime descriptor");
    }

    None
}

fn token_character(character: char) -> bool {
    character.is_ascii_alphanumeric() || matches!(character, '_' | '-')
}

fn collect_value(remainder: &str, accepted: impl Fn(char) -> bool) -> String {
    remainder.chars().take_while(|c| accepted(*c)).collect()
}

fn value_after(
    line: &str,
    prefix: &str,
    minimum: usize,
    accepted: impl Fn(char) -> bool + Copy,
) -> Option<String> {
    let index = line.find(prefix)?;
    let value = collect_value(&line[index + prefix.len()..], accepted);
    (value.len() >= minimum).then_some(value)
}

fn private_key_block(contents: &str) -> Option<u32> {
    let lines: Vec<&str> = contents.lines().collect();
    for (index, line) in lines.iter().enumerate() {
        let Some(rest) = line.trim().strip_prefix("-----BEGIN ") else {
            continue;
        };
        let Some(kind) = rest.strip_suffix(" PRIVATE KEY-----") else {
            continue;
        };
        if kind.is_empty()
            || !kind
                .chars()
                .all(|character| character.is_ascii_uppercase() || character == ' ')
        {
            continue;
        }
        let body: usize = lines
            .iter()
            .skip(index + 1)
            .take_while(|candidate| !candidate.trim().starts_with("-----END "))
            .map(|candidate| {
                candidate
                    .trim()
                    .chars()
                    .filter(|character| {
                        character.is_ascii_alphanumeric() || matches!(character, '+' | '/' | '=')
                    })
                    .count()
            })
            .sum();
        if body >= 40 {
            return Some(index as u32 + 1);
        }
    }
    None
}

pub fn looks_random(value: &str) -> bool {
    let uppercased = value.to_ascii_uppercase();
    for marker in ["EXAMPLE", "SAMPLE", "PLACEHOLDER", "REDACTED", "XXXXXX"] {
        if uppercased.contains(marker) {
            return false;
        }
    }
    let characters: Vec<char> = value.chars().collect();
    if characters.len() < 8 {
        return false;
    }
    let distinct = characters
        .iter()
        .map(|character| character.to_ascii_lowercase())
        .collect::<std::collections::BTreeSet<char>>()
        .len();
    if distinct < 8 {
        return false;
    }
    let ascending = characters.windows(2).all(|pair| pair[0] <= pair[1]);
    let descending = characters.windows(2).all(|pair| pair[0] >= pair[1]);
    !ascending && !descending
}
