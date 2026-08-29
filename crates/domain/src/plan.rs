use serde::{Deserialize, Serialize};

use crate::clock::Timestamp;
use crate::error::DomainError;
use crate::identity::{ApprovalId, PlanHash};

pub const REQUIRED_PLAN_HEADINGS: [&str; 10] = [
    "Task interpretation",
    "Current repository findings",
    "Assumptions",
    "Proposed design",
    "Files expected to change",
    "Compatibility and migration",
    "Test strategy",
    "Quality and security checks",
    "Risks and mitigations",
    "Acceptance checklist",
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PlanVersion {
    pub version: u32,
    pub hash: PlanHash,
    pub created_at: Timestamp,
    pub author: PlanAuthor,
    pub revision_note: Option<String>,
    pub byte_length: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PlanAuthor {
    Agent,
    Human,
}

impl PlanAuthor {
    pub fn as_str(&self) -> &'static str {
        match self {
            PlanAuthor::Agent => "agent",
            PlanAuthor::Human => "human",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalDecision {
    Approved,
    RevisionRequested,
    Rejected,
}

impl ApprovalDecision {
    pub fn as_str(&self) -> &'static str {
        match self {
            ApprovalDecision::Approved => "approved",
            ApprovalDecision::RevisionRequested => "revision_requested",
            ApprovalDecision::Rejected => "rejected",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PlanApproval {
    pub id: ApprovalId,
    pub decision: ApprovalDecision,
    pub plan_version: u32,
    pub plan_hash: PlanHash,
    pub decided_at: Timestamp,
    pub local_user: String,
    pub note: Option<String>,
}

impl PlanApproval {
    pub fn is_valid_for(&self, current_hash: &PlanHash) -> bool {
        self.decision == ApprovalDecision::Approved && &self.plan_hash == current_hash
    }

    pub fn ensure_valid_for(&self, current_hash: &PlanHash) -> Result<(), DomainError> {
        if self.is_valid_for(current_hash) {
            Ok(())
        } else {
            Err(DomainError::ApprovalHashMismatch {
                approved: self.plan_hash.to_string(),
                current: current_hash.to_string(),
            })
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, Default)]
pub struct PlanHistory {
    pub versions: Vec<PlanVersion>,
    pub approval: Option<PlanApproval>,
}

impl PlanHistory {
    pub fn current(&self) -> Option<&PlanVersion> {
        self.versions.last()
    }

    pub fn next_version_number(&self) -> u32 {
        self.current()
            .map(|version| version.version + 1)
            .unwrap_or(1)
    }

    pub fn version(&self, number: u32) -> Option<&PlanVersion> {
        self.versions
            .iter()
            .find(|version| version.version == number)
    }

    pub fn approved_hash(&self) -> Option<&PlanHash> {
        let approval = self.approval.as_ref()?;
        let current = self.current()?;
        if approval.is_valid_for(&current.hash) {
            Some(&approval.plan_hash)
        } else {
            None
        }
    }

    pub fn is_approved(&self) -> bool {
        self.approved_hash().is_some()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PlanValidation {
    pub missing_headings: Vec<String>,
    pub empty_sections: Vec<String>,
    pub expected_files: Vec<String>,
}

impl PlanValidation {
    pub fn is_acceptable(&self) -> bool {
        self.missing_headings.is_empty()
    }
}

pub fn validate_plan_document(markdown: &str) -> PlanValidation {
    let mut missing_headings = Vec::new();
    let mut empty_sections = Vec::new();
    let sections = split_sections(markdown);

    for heading in REQUIRED_PLAN_HEADINGS {
        match sections
            .iter()
            .find(|section| section.title.eq_ignore_ascii_case(heading))
        {
            Some(section) => {
                if section.body.trim().is_empty() {
                    empty_sections.push(heading.to_string());
                }
            }
            None => missing_headings.push(heading.to_string()),
        }
    }

    let expected_files = sections
        .iter()
        .find(|section| {
            section
                .title
                .eq_ignore_ascii_case("Files expected to change")
        })
        .map(|section| extract_list_items(&section.body))
        .unwrap_or_default();

    PlanValidation {
        missing_headings,
        empty_sections,
        expected_files,
    }
}

struct PlanSection {
    title: String,
    body: String,
}

fn split_sections(markdown: &str) -> Vec<PlanSection> {
    let mut sections: Vec<PlanSection> = Vec::new();
    let mut current: Option<PlanSection> = None;
    let mut in_fence = false;

    for line in markdown.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") {
            in_fence = !in_fence;
        }
        if !in_fence && trimmed.starts_with('#') {
            if let Some(section) = current.take() {
                sections.push(section);
            }
            let title = trimmed.trim_start_matches('#').trim().to_string();
            current = Some(PlanSection {
                title,
                body: String::new(),
            });
            continue;
        }
        if let Some(section) = current.as_mut() {
            section.body.push_str(line);
            section.body.push('\n');
        }
    }
    if let Some(section) = current.take() {
        sections.push(section);
    }
    sections
}

fn extract_list_items(body: &str) -> Vec<String> {
    body.lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            let stripped = trimmed
                .strip_prefix("- ")
                .or_else(|| trimmed.strip_prefix("* "))
                .or_else(|| trimmed.strip_prefix("+ "))?;
            let cleaned = stripped.trim().trim_matches('`').trim();
            if cleaned.is_empty() {
                None
            } else {
                Some(cleaned.to_string())
            }
        })
        .collect()
}
