use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::error::DomainError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FailureClass {
    TaskFailure,
    TransientInfrastructure,
    PermanentConfiguration,
    PolicyViolation,
    UserActionRequired,
    Cancelled,
    InternalInvariant,
}

impl FailureClass {
    pub const ALL: [FailureClass; 7] = [
        FailureClass::TaskFailure,
        FailureClass::TransientInfrastructure,
        FailureClass::PermanentConfiguration,
        FailureClass::PolicyViolation,
        FailureClass::UserActionRequired,
        FailureClass::Cancelled,
        FailureClass::InternalInvariant,
    ];

    pub fn as_str(&self) -> &'static str {
        match self {
            FailureClass::TaskFailure => "task_failure",
            FailureClass::TransientInfrastructure => "transient_infrastructure",
            FailureClass::PermanentConfiguration => "permanent_configuration",
            FailureClass::PolicyViolation => "policy_violation",
            FailureClass::UserActionRequired => "user_action_required",
            FailureClass::Cancelled => "cancelled",
            FailureClass::InternalInvariant => "internal_invariant",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            FailureClass::TaskFailure => "Task failure",
            FailureClass::TransientInfrastructure => "Transient infrastructure failure",
            FailureClass::PermanentConfiguration => "Permanent configuration failure",
            FailureClass::PolicyViolation => "Policy violation",
            FailureClass::UserActionRequired => "User action required",
            FailureClass::Cancelled => "Cancelled",
            FailureClass::InternalInvariant => "Internal invariant violation",
        }
    }

    pub fn is_automatically_retryable(&self) -> bool {
        matches!(self, FailureClass::TransientInfrastructure)
    }

    pub fn routes_to_repair(&self) -> bool {
        matches!(self, FailureClass::TaskFailure)
    }

    pub fn requires_user_action(&self) -> bool {
        matches!(self, FailureClass::UserActionRequired)
    }
}

impl fmt::Display for FailureClass {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for FailureClass {
    type Err = DomainError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        FailureClass::ALL
            .into_iter()
            .find(|class| class.as_str() == value)
            .ok_or_else(|| DomainError::InvalidIdentifier {
                kind: "FailureClass",
                value: value.to_string(),
            })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct NodeFailure {
    pub class: FailureClass,
    pub code: String,
    pub message: String,
    pub remedy: Option<String>,
    pub evidence_reference: Option<String>,
    pub fingerprint: Option<String>,
}

impl NodeFailure {
    pub fn new(class: FailureClass, code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            class,
            code: code.into(),
            message: message.into(),
            remedy: None,
            evidence_reference: None,
            fingerprint: None,
        }
    }

    pub fn with_remedy(mut self, remedy: impl Into<String>) -> Self {
        self.remedy = Some(remedy.into());
        self
    }

    pub fn with_evidence(mut self, reference: impl Into<String>) -> Self {
        self.evidence_reference = Some(reference.into());
        self
    }

    pub fn with_fingerprint(mut self, fingerprint: impl Into<String>) -> Self {
        self.fingerprint = Some(fingerprint.into());
        self
    }
}

impl fmt::Display for NodeFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "[{}] {}: {}", self.class, self.code, self.message)
    }
}
