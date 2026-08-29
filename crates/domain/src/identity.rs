use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::DomainError;

macro_rules! sortable_uuid_id {
    ($name:ident, $label:literal) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(Uuid);

        impl $name {
            pub fn from_uuid(value: Uuid) -> Self {
                Self(value)
            }

            pub fn as_uuid(&self) -> Uuid {
                self.0
            }

            pub fn short(&self) -> String {
                self.0.simple().to_string()[..12].to_string()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(formatter, "{}", self.0.hyphenated())
            }
        }

        impl FromStr for $name {
            type Err = DomainError;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                let parsed = Uuid::parse_str(value).map_err(|_| DomainError::InvalidIdentifier {
                    kind: $label,
                    value: value.to_string(),
                })?;
                Ok(Self(parsed))
            }
        }

        impl schemars::JsonSchema for $name {
            fn schema_name() -> String {
                $label.to_string()
            }

            fn json_schema(generator: &mut schemars::gen::SchemaGenerator) -> schemars::schema::Schema {
                let mut schema = <String as schemars::JsonSchema>::json_schema(generator).into_object();
                schema.format = Some("uuid".to_string());
                schema.into()
            }
        }
    };
}

sortable_uuid_id!(RunId, "RunId");
sortable_uuid_id!(EventId, "EventId");
sortable_uuid_id!(ApprovalId, "ApprovalId");

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CandidateOrdinal(u8);

impl CandidateOrdinal {
    pub const MAX: u8 = 8;

    pub fn new(value: u8) -> Result<Self, DomainError> {
        if value == 0 || value > Self::MAX {
            return Err(DomainError::InvalidIdentifier {
                kind: "CandidateOrdinal",
                value: value.to_string(),
            });
        }
        Ok(Self(value))
    }

    pub fn get(&self) -> u8 {
        self.0
    }
}

impl schemars::JsonSchema for CandidateOrdinal {
    fn schema_name() -> String {
        "CandidateOrdinal".to_string()
    }

    fn json_schema(generator: &mut schemars::gen::SchemaGenerator) -> schemars::schema::Schema {
        <u8 as schemars::JsonSchema>::json_schema(generator)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CandidateId(String);

impl CandidateId {
    pub fn derive(run: RunId, ordinal: CandidateOrdinal) -> Self {
        Self(format!("c{:02}-{}", ordinal.get(), &run.short()[..8]))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn ordinal(&self) -> Option<CandidateOrdinal> {
        let digits = self.0.strip_prefix('c')?.get(..2)?;
        let parsed = digits.parse::<u8>().ok()?;
        CandidateOrdinal::new(parsed).ok()
    }
}

impl fmt::Display for CandidateId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl FromStr for CandidateId {
    type Err = DomainError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let valid = value.len() >= 4
            && value.len() <= 32
            && value.starts_with('c')
            && value
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || character == '-');
        if !valid {
            return Err(DomainError::InvalidIdentifier {
                kind: "CandidateId",
                value: value.to_string(),
            });
        }
        Ok(Self(value.to_string()))
    }
}

impl schemars::JsonSchema for CandidateId {
    fn schema_name() -> String {
        "CandidateId".to_string()
    }

    fn json_schema(generator: &mut schemars::gen::SchemaGenerator) -> schemars::schema::Schema {
        <String as schemars::JsonSchema>::json_schema(generator)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AttemptNumber(u32);

impl AttemptNumber {
    pub const FIRST: AttemptNumber = AttemptNumber(1);

    pub fn new(value: u32) -> Result<Self, DomainError> {
        if value == 0 {
            return Err(DomainError::InvalidIdentifier {
                kind: "AttemptNumber",
                value: value.to_string(),
            });
        }
        Ok(Self(value))
    }

    pub fn get(&self) -> u32 {
        self.0
    }

    pub fn next(&self) -> Self {
        Self(self.0.saturating_add(1))
    }
}

impl fmt::Display for AttemptNumber {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.0)
    }
}

impl schemars::JsonSchema for AttemptNumber {
    fn schema_name() -> String {
        "AttemptNumber".to_string()
    }

    fn json_schema(generator: &mut schemars::gen::SchemaGenerator) -> schemars::schema::Schema {
        <u32 as schemars::JsonSchema>::json_schema(generator)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct AttemptId {
    pub node: crate::graph::NodeId,
    pub candidate: Option<CandidateId>,
    pub attempt: AttemptNumber,
}

impl AttemptId {
    pub fn new(node: crate::graph::NodeId, candidate: Option<CandidateId>, attempt: AttemptNumber) -> Self {
        Self {
            node,
            candidate,
            attempt,
        }
    }
}

impl fmt::Display for AttemptId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.candidate {
            Some(candidate) => write!(formatter, "{}/{}/{}", candidate, self.node, self.attempt),
            None => write!(formatter, "{}/{}", self.node, self.attempt),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ContentDigest(String);

impl ContentDigest {
    pub fn of_bytes(bytes: &[u8]) -> Self {
        Self(blake3::hash(bytes).to_hex().to_string())
    }

    pub fn of_str(value: &str) -> Self {
        Self::of_bytes(value.as_bytes())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn short(&self) -> &str {
        &self.0[..16]
    }
}

impl fmt::Display for ContentDigest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl FromStr for ContentDigest {
    type Err = DomainError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let valid = value.len() == 64 && value.chars().all(|character| character.is_ascii_hexdigit());
        if !valid {
            return Err(DomainError::InvalidIdentifier {
                kind: "ContentDigest",
                value: value.to_string(),
            });
        }
        Ok(Self(value.to_ascii_lowercase()))
    }
}

impl schemars::JsonSchema for ContentDigest {
    fn schema_name() -> String {
        "ContentDigest".to_string()
    }

    fn json_schema(generator: &mut schemars::gen::SchemaGenerator) -> schemars::schema::Schema {
        <String as schemars::JsonSchema>::json_schema(generator)
    }
}

pub type ArtifactId = ContentDigest;
pub type PlanHash = ContentDigest;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CommitHash(String);

impl CommitHash {
    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn short(&self) -> &str {
        &self.0[..self.0.len().min(12)]
    }
}

impl fmt::Display for CommitHash {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl FromStr for CommitHash {
    type Err = DomainError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let valid = (7..=64).contains(&value.len()) && value.chars().all(|character| character.is_ascii_hexdigit());
        if !valid {
            return Err(DomainError::InvalidIdentifier {
                kind: "CommitHash",
                value: value.to_string(),
            });
        }
        Ok(Self(value.to_ascii_lowercase()))
    }
}

impl schemars::JsonSchema for CommitHash {
    fn schema_name() -> String {
        "CommitHash".to_string()
    }

    fn json_schema(generator: &mut schemars::gen::SchemaGenerator) -> schemars::schema::Schema {
        <String as schemars::JsonSchema>::json_schema(generator)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct BranchName(String);

impl BranchName {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for BranchName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl FromStr for BranchName {
    type Err = DomainError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let forbidden_sequences = ["..", "@{", "//", "\\"];
        let has_forbidden_sequence = forbidden_sequences
            .iter()
            .any(|sequence| value.contains(sequence));
        let has_forbidden_character = value.chars().any(|character| {
            character.is_control()
                || matches!(character, ' ' | '~' | '^' | ':' | '?' | '*' | '[')
        });
        let valid = !value.is_empty()
            && value.len() <= 200
            && !value.starts_with('-')
            && !value.starts_with('/')
            && !value.ends_with('/')
            && !value.ends_with(".lock")
            && !has_forbidden_sequence
            && !has_forbidden_character;
        if !valid {
            return Err(DomainError::InvalidIdentifier {
                kind: "BranchName",
                value: value.to_string(),
            });
        }
        Ok(Self(value.to_string()))
    }
}

impl schemars::JsonSchema for BranchName {
    fn schema_name() -> String {
        "BranchName".to_string()
    }

    fn json_schema(generator: &mut schemars::gen::SchemaGenerator) -> schemars::schema::Schema {
        <String as schemars::JsonSchema>::json_schema(generator)
    }
}
