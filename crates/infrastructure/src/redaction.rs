use std::collections::BTreeSet;

use heikas_application::configuration::RedactionConfiguration;
use heikas_application::ports::observability::Redactor;
use regex::Regex;
use serde_json::Value;

pub const REDACTION_PLACEHOLDER: &str = "[redacted]";

pub struct PatternRedactor {
    literals: Vec<String>,
    patterns: Vec<Regex>,
    home_prefix: Option<String>,
}

impl PatternRedactor {
    pub fn new(
        secret_environment_variables: &[String],
        additional_patterns: &[String],
        home_prefix: Option<String>,
    ) -> Self {
        let mut literals: BTreeSet<String> = BTreeSet::new();
        for name in secret_environment_variables {
            if let Ok(value) = std::env::var(name) {
                let trimmed = value.trim();
                if trimmed.len() >= 8 {
                    literals.insert(trimmed.to_string());
                }
            }
        }
        let mut patterns = default_patterns();
        for pattern in additional_patterns {
            if let Ok(compiled) = Regex::new(pattern) {
                patterns.push(compiled);
            }
        }
        Self {
            literals: literals.into_iter().collect(),
            patterns,
            home_prefix,
        }
    }

    pub fn for_configuration(configuration: &RedactionConfiguration) -> Self {
        let home = if configuration.redact_home_prefix {
            std::env::var("HOME")
                .ok()
                .or_else(|| std::env::var("USERPROFILE").ok())
        } else {
            None
        };
        Self::new(
            &configuration.secret_environment_variables,
            &configuration.additional_patterns,
            home,
        )
    }

    pub fn without_environment() -> Self {
        Self {
            literals: Vec::new(),
            patterns: default_patterns(),
            home_prefix: None,
        }
    }

    fn redact(&self, value: &str) -> String {
        let mut redacted = value.to_string();
        for literal in &self.literals {
            if redacted.contains(literal.as_str()) {
                redacted = redacted.replace(literal.as_str(), REDACTION_PLACEHOLDER);
            }
        }
        for pattern in &self.patterns {
            redacted = pattern
                .replace_all(&redacted, REDACTION_PLACEHOLDER)
                .into_owned();
        }
        if let Some(home) = &self.home_prefix {
            if !home.is_empty() && redacted.contains(home.as_str()) {
                redacted = redacted.replace(home.as_str(), "~");
            }
        }
        redacted
    }
}

impl Redactor for PatternRedactor {
    fn redact_text(&self, value: &str) -> String {
        self.redact(value)
    }

    fn redact_bytes(&self, value: &[u8]) -> Vec<u8> {
        match std::str::from_utf8(value) {
            Ok(text) => self.redact(text).into_bytes(),
            Err(_) => value.to_vec(),
        }
    }

    fn redact_json(&self, value: &serde_json::Value) -> serde_json::Value {
        match value {
            serde_json::Value::String(text) => serde_json::Value::String(self.redact(text)),
            serde_json::Value::Array(items) => {
                serde_json::Value::Array(items.iter().map(|item| self.redact_json(item)).collect())
            }
            serde_json::Value::Object(entries) => {
                let mut mapped = serde_json::Map::new();
                for (key, entry) in entries {
                    if is_sensitive_key(key) {
                        mapped.insert(
                            key.clone(),
                            serde_json::Value::String(REDACTION_PLACEHOLDER.to_string()),
                        );
                    } else {
                        mapped.insert(key.clone(), self.redact_json(entry));
                    }
                }
                serde_json::Value::Object(mapped)
            }
            other => other.clone(),
        }
    }
}

fn is_sensitive_key(key: &str) -> bool {
    let lowered = key.to_ascii_lowercase();
    [
        "password",
        "passphrase",
        "secret",
        "token",
        "api_key",
        "apikey",
        "authorization",
        "credential",
        "private_key",
        "session",
    ]
    .iter()
    .any(|needle| lowered.contains(needle))
}

fn default_patterns() -> Vec<Regex> {
    [
        r"gh[pousr]_[A-Za-z0-9]{16,}",
        r"github_pat_[A-Za-z0-9_]{20,}",
        r"sk-[A-Za-z0-9\-_]{16,}",
        r"sk-ant-[A-Za-z0-9\-_]{16,}",
        r"xox[baprs]-[A-Za-z0-9\-]{10,}",
        r"AKIA[0-9A-Z]{16}",
        r"AIza[0-9A-Za-z\-_]{35}",
        r"-----BEGIN [A-Z ]*PRIVATE KEY-----[\s\S]*?-----END [A-Z ]*PRIVATE KEY-----",
        r"eyJ[A-Za-z0-9_\-]{10,}\.[A-Za-z0-9_\-]{10,}\.[A-Za-z0-9_\-]{10,}",
        r"(?i)\b(?:authorization|api[-_]?key|token|secret|password|passphrase|credential)\s*[:=]\s*(?:bearer\s+|token\s+|basic\s+)?['\x22]?[A-Za-z0-9/+_\-\.=]{12,}['\x22]?",
        r"(?i)\b(?:bearer|basic)\s+[A-Za-z0-9\-._~+/]{12,}={0,2}",
        r"[A-Za-z][A-Za-z0-9+.\-]*://[^\s/@:]+:[^\s/@]+@[^\s]+",
    ]
    .into_iter()
    .filter_map(|pattern| Regex::new(pattern).ok())
    .collect()
}

pub fn redact_text_leaves(redactor: &dyn Redactor, value: &Value) -> Value {
    match value {
        Value::String(text) => Value::String(redactor.redact_text(text)),
        Value::Array(items) => Value::Array(
            items
                .iter()
                .map(|item| redact_text_leaves(redactor, item))
                .collect(),
        ),
        Value::Object(entries) => {
            let mut mapped = serde_json::Map::with_capacity(entries.len());
            for (key, entry) in entries {
                mapped.insert(key.clone(), redact_text_leaves(redactor, entry));
            }
            Value::Object(mapped)
        }
        other => other.clone(),
    }
}
