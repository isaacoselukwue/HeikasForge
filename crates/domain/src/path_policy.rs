use std::fmt;

use serde::{Deserialize, Serialize};

use crate::error::DomainError;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RelativeWorkspacePath(String);

impl RelativeWorkspacePath {
    pub fn parse(raw: &str) -> Result<Self, DomainError> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Err(DomainError::MissingField { field: "path" });
        }
        if trimmed.contains('\0') || trimmed.contains('\n') || trimmed.contains('\r') {
            return Err(DomainError::PathEscapesWorktree {
                path: raw.to_string(),
            });
        }
        let normalised = trimmed.replace('\\', "/");
        if normalised.starts_with('/') || normalised.starts_with("~") {
            return Err(DomainError::PathEscapesWorktree {
                path: raw.to_string(),
            });
        }
        if has_windows_drive_prefix(&normalised) {
            return Err(DomainError::PathEscapesWorktree {
                path: raw.to_string(),
            });
        }
        let mut segments: Vec<&str> = Vec::new();
        for segment in normalised.split('/') {
            match segment {
                "" | "." => continue,
                ".." => {
                    return Err(DomainError::PathEscapesWorktree {
                        path: raw.to_string(),
                    })
                }
                other => segments.push(other),
            }
        }
        if segments.is_empty() {
            return Err(DomainError::PathEscapesWorktree {
                path: raw.to_string(),
            });
        }
        Ok(Self(segments.join("/")))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn segments(&self) -> std::str::Split<'_, char> {
        self.0.split('/')
    }

    pub fn file_name(&self) -> &str {
        self.0.rsplit('/').next().unwrap_or(&self.0)
    }

    pub fn extension(&self) -> Option<&str> {
        let name = self.file_name();
        let index = name.rfind('.')?;
        if index == 0 {
            return None;
        }
        Some(&name[index + 1..])
    }

    pub fn starts_with_segment(&self, segment: &str) -> bool {
        self.segments().next() == Some(segment)
    }
}

impl fmt::Display for RelativeWorkspacePath {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl schemars::JsonSchema for RelativeWorkspacePath {
    fn schema_name() -> String {
        "RelativeWorkspacePath".to_string()
    }

    fn json_schema(generator: &mut schemars::gen::SchemaGenerator) -> schemars::schema::Schema {
        <String as schemars::JsonSchema>::json_schema(generator)
    }
}

fn has_windows_drive_prefix(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':'
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PathAccess {
    Read,
    Write,
    Delete,
}

impl PathAccess {
    pub fn as_str(&self) -> &'static str {
        match self {
            PathAccess::Read => "read",
            PathAccess::Write => "write",
            PathAccess::Delete => "delete",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PathPolicy {
    pub protected_patterns: Vec<String>,
    pub sensitive_patterns: Vec<String>,
    pub approved_protected_paths: Vec<String>,
    pub maximum_read_bytes: u64,
    pub maximum_write_bytes: u64,
}

impl Default for PathPolicy {
    fn default() -> Self {
        Self {
            protected_patterns: default_protected_patterns()
                .into_iter()
                .map(str::to_string)
                .collect(),
            sensitive_patterns: default_sensitive_patterns()
                .into_iter()
                .map(str::to_string)
                .collect(),
            approved_protected_paths: Vec::new(),
            maximum_read_bytes: 1_048_576,
            maximum_write_bytes: 4_194_304,
        }
    }
}

pub fn default_protected_patterns() -> Vec<&'static str> {
    vec![
        ".git/**",
        ".git",
        ".githooks/**",
        ".github/workflows/**",
        ".heikas/**",
        ".gitmodules",
        ".gitattributes",
    ]
}

pub fn default_sensitive_patterns() -> Vec<&'static str> {
    vec![
        "**/.env",
        "**/.env.*",
        "**/*.pem",
        "**/*.key",
        "**/*.p12",
        "**/*.pfx",
        "**/id_rsa",
        "**/id_ed25519",
        "**/.ssh/**",
        "**/.aws/**",
        "**/.gnupg/**",
        "**/.npmrc",
        "**/.netrc",
        "**/.docker/config.json",
        "**/.kube/config",
        "**/secrets.*",
        "**/*.keystore",
    ]
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PathDecision {
    pub path: RelativeWorkspacePath,
    pub access: PathAccess,
    pub permitted: bool,
    pub reason: Option<String>,
}

pub trait PatternMatcher {
    fn matches(&self, pattern: &str, path: &str) -> bool;
}

pub fn evaluate_path<M: PatternMatcher>(
    policy: &PathPolicy,
    matcher: &M,
    path: &RelativeWorkspacePath,
    access: PathAccess,
) -> Result<(), DomainError> {
    for pattern in &policy.sensitive_patterns {
        if matcher.matches(pattern, path.as_str()) {
            return Err(DomainError::PathSensitive {
                path: path.to_string(),
                pattern: pattern.clone(),
            });
        }
    }
    if access == PathAccess::Read {
        return Ok(());
    }
    let approved = policy
        .approved_protected_paths
        .iter()
        .any(|approved_path| approved_path == path.as_str());
    if approved {
        return Ok(());
    }
    for pattern in &policy.protected_patterns {
        if matcher.matches(pattern, path.as_str()) {
            return Err(DomainError::PathProtected {
                path: path.to_string(),
                pattern: pattern.clone(),
            });
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum WorktreeRole {
    Candidate,
    Integration,
    Source,
}

impl WorktreeRole {
    pub fn as_str(&self) -> &'static str {
        match self {
            WorktreeRole::Candidate => "candidate",
            WorktreeRole::Integration => "integration",
            WorktreeRole::Source => "source",
        }
    }

    pub fn permits_write(&self) -> bool {
        matches!(self, WorktreeRole::Candidate | WorktreeRole::Integration)
    }
}
