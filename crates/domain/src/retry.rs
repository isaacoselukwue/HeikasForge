use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::failure::FailureClass;
use crate::graph::{NodeClass, NodeId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RetryPolicy {
    pub maximum_attempts: u32,
    pub initial_delay_ms: u64,
    pub multiplier: u32,
    pub maximum_delay_ms: u64,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            maximum_attempts: 3,
            initial_delay_ms: 500,
            multiplier: 2,
            maximum_delay_ms: 8_000,
        }
    }
}

impl RetryPolicy {
    pub fn base_delay(&self, attempt: u32) -> Duration {
        let exponent = attempt.saturating_sub(1).min(16);
        let multiplier = u64::from(self.multiplier).saturating_pow(exponent);
        let delay = self.initial_delay_ms.saturating_mul(multiplier);
        Duration::from_millis(delay.min(self.maximum_delay_ms))
    }

    pub fn delay_with_jitter(&self, attempt: u32, jitter_fraction: f64) -> Duration {
        let base = self.base_delay(attempt).as_millis() as f64;
        let bounded = jitter_fraction.clamp(0.0, 1.0);
        Duration::from_millis((base * bounded) as u64)
    }

    pub fn permits_attempt(&self, attempt: u32) -> bool {
        attempt < self.maximum_attempts
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RetryDecision {
    RetrySameNode,
    RouteToRepair,
    PauseForUser,
    FailRun,
    FailCandidate,
    Cancel,
}

impl RetryDecision {
    pub fn as_str(&self) -> &'static str {
        match self {
            RetryDecision::RetrySameNode => "retry_same_node",
            RetryDecision::RouteToRepair => "route_to_repair",
            RetryDecision::PauseForUser => "pause_for_user",
            RetryDecision::FailRun => "fail_run",
            RetryDecision::FailCandidate => "fail_candidate",
            RetryDecision::Cancel => "cancel",
        }
    }
}

pub fn classify_retry(
    node: NodeId,
    class: FailureClass,
    attempt: u32,
    policy: RetryPolicy,
    repair_budget_remaining: bool,
) -> RetryDecision {
    match class {
        FailureClass::Cancelled => RetryDecision::Cancel,
        FailureClass::UserActionRequired => RetryDecision::PauseForUser,
        FailureClass::TransientInfrastructure => {
            if policy.permits_attempt(attempt) {
                RetryDecision::RetrySameNode
            } else if node.scope() == crate::graph::NodeScope::Candidate {
                RetryDecision::FailCandidate
            } else {
                RetryDecision::FailRun
            }
        }
        FailureClass::TaskFailure => {
            if node.scope() == crate::graph::NodeScope::Candidate && repair_budget_remaining {
                RetryDecision::RouteToRepair
            } else if node.scope() == crate::graph::NodeScope::Candidate {
                RetryDecision::FailCandidate
            } else {
                RetryDecision::FailRun
            }
        }
        FailureClass::PolicyViolation | FailureClass::PermanentConfiguration => {
            if node.scope() == crate::graph::NodeScope::Candidate {
                RetryDecision::FailCandidate
            } else {
                RetryDecision::FailRun
            }
        }
        FailureClass::InternalInvariant => RetryDecision::FailRun,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct NodeTimeouts {
    pub agent_seconds: u32,
    pub command_seconds: u32,
    pub review_seconds: u32,
    pub git_seconds: u32,
}

impl Default for NodeTimeouts {
    fn default() -> Self {
        Self {
            agent_seconds: 1_200,
            command_seconds: 900,
            review_seconds: 600,
            git_seconds: 120,
        }
    }
}

impl NodeTimeouts {
    pub fn for_node(&self, node: NodeId) -> Duration {
        let seconds = match node.class() {
            NodeClass::Agent => self.agent_seconds,
            NodeClass::Command => self.command_seconds,
            NodeClass::Review => self.review_seconds,
            NodeClass::Git => self.git_seconds,
            NodeClass::Preparation | NodeClass::Decision => self.git_seconds,
        };
        Duration::from_secs(u64::from(seconds))
    }
}
