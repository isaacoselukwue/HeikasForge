use crate::error::PolicyResult;
use crate::finding::PolicyFinding;
use crate::repository::TrackedRepository;
use crate::rules::is_tracked_text;

pub const HOST_PATH_RULE: &str = "leakage.no-host-paths";
pub const SECRET_RULE: &str = "leakage.no-secret-material";

pub const PLACEHOLDER_ACCOUNTS: [&str; 8] = [
    "you", "operator", "user", "username", "runner", "ci", "example", "someone",
];

const DATA_EXTENSIONS: [&str; 9] = [
    "json", "jsonl", "yaml", "yml", "toml", "env", "log", "cfg", "ini",
];

pub fn check(repository: &TrackedRepository) -> PolicyResult<Vec<PolicyFinding>> {
    let mut findings = Vec::new();
    for path in &repository.tracked_files {
        if !is_tracked_text(path) || path == crate::rules::spelling::DICTIONARY_PATH {
            continue;
        }
        let Some(contents) = repository.read_text(path)? else {
            continue;
        };
        let is_data = DATA_EXTENSIONS.contains(&path.rsplit('.').next().unwrap_or(""));
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
            if is_data {
                if let Some(shape) = secret_shape(line) {
                    findings.push(
                        PolicyFinding::violation(
                            SECRET_RULE,
                            format!("a tracked data file contains {shape}"),
                            "Remove the file from version control and add it to the ignore rules.",
                        )
                        .at(path.clone(), index as u32 + 1, 1),
                    );
                }
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
            let lowered = account.to_ascii_lowercase();
            if PLACEHOLDER_ACCOUNTS.contains(&lowered.as_str()) {
                continue;
            }
            if lowered.starts_with('<') || lowered.starts_with('$') || lowered.starts_with('{') {
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
        let value: String = line[index + "token=".len()..]
            .chars()
            .take_while(|character| character.is_ascii_hexdigit())
            .collect();
        if value.len() >= 32 {
            return Some("a session token");
        }
    }
    if line.contains("-----BEGIN ") && line.contains("PRIVATE KEY-----") {
        return Some("a private key block");
    }
    for prefix in ["ghp_", "gho_", "ghu_", "ghs_", "ghr_", "github_pat_"] {
        if let Some(index) = line.find(prefix) {
            let value: String = line[index + prefix.len()..]
                .chars()
                .take_while(|character| character.is_ascii_alphanumeric() || *character == '_')
                .collect();
            if value.len() >= 20 {
                return Some("a source forge access token");
            }
        }
    }
    if lowered.contains("\"serverpid\"") || lowered.contains("\"bootstrapurl\"") {
        return Some("a captured runtime descriptor");
    }
    None
}
