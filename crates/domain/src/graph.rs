use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::error::DomainError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum NodeId {
    Prepare,
    Plan,
    Approval,
    FanOut,
    ImplementCandidate,
    TestCandidate,
    ReviewCandidate,
    RepairCandidate,
    Join,
    IntegrateWinner,
    FinalTest,
    FinalReview,
    CommitApproval,
    Commit,
}

impl NodeId {
    pub const ALL: [NodeId; 14] = [
        NodeId::Prepare,
        NodeId::Plan,
        NodeId::Approval,
        NodeId::FanOut,
        NodeId::ImplementCandidate,
        NodeId::TestCandidate,
        NodeId::ReviewCandidate,
        NodeId::RepairCandidate,
        NodeId::Join,
        NodeId::IntegrateWinner,
        NodeId::FinalTest,
        NodeId::FinalReview,
        NodeId::CommitApproval,
        NodeId::Commit,
    ];

    pub fn as_str(&self) -> &'static str {
        match self {
            NodeId::Prepare => "prepare",
            NodeId::Plan => "plan",
            NodeId::Approval => "approval",
            NodeId::FanOut => "fan_out",
            NodeId::ImplementCandidate => "implement_candidate",
            NodeId::TestCandidate => "test_candidate",
            NodeId::ReviewCandidate => "review_candidate",
            NodeId::RepairCandidate => "repair_candidate",
            NodeId::Join => "join",
            NodeId::IntegrateWinner => "integrate_winner",
            NodeId::FinalTest => "final_test",
            NodeId::FinalReview => "final_review",
            NodeId::CommitApproval => "commit_approval",
            NodeId::Commit => "commit",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            NodeId::Prepare => "Prepare",
            NodeId::Plan => "Plan",
            NodeId::Approval => "Plan approval",
            NodeId::FanOut => "Fan out",
            NodeId::ImplementCandidate => "Implement",
            NodeId::TestCandidate => "Test",
            NodeId::ReviewCandidate => "Review",
            NodeId::RepairCandidate => "Repair",
            NodeId::Join => "Join",
            NodeId::IntegrateWinner => "Integrate winner",
            NodeId::FinalTest => "Final test",
            NodeId::FinalReview => "Final review",
            NodeId::CommitApproval => "Commit approval",
            NodeId::Commit => "Commit",
        }
    }

    pub fn scope(&self) -> NodeScope {
        match self {
            NodeId::ImplementCandidate
            | NodeId::TestCandidate
            | NodeId::ReviewCandidate
            | NodeId::RepairCandidate => NodeScope::Candidate,
            _ => NodeScope::Run,
        }
    }

    pub fn is_read_only(&self) -> bool {
        matches!(
            self,
            NodeId::Prepare | NodeId::Plan | NodeId::Approval | NodeId::Join | NodeId::CommitApproval
        )
    }

    pub fn allowed_successors(&self) -> &'static [NodeId] {
        match self {
            NodeId::Prepare => &[NodeId::Plan],
            NodeId::Plan => &[NodeId::Approval],
            NodeId::Approval => &[NodeId::FanOut, NodeId::Plan],
            NodeId::FanOut => &[NodeId::ImplementCandidate],
            NodeId::ImplementCandidate => &[NodeId::TestCandidate],
            NodeId::TestCandidate => &[NodeId::ReviewCandidate, NodeId::RepairCandidate, NodeId::Join],
            NodeId::ReviewCandidate => &[NodeId::RepairCandidate, NodeId::Join],
            NodeId::RepairCandidate => &[NodeId::TestCandidate, NodeId::Join],
            NodeId::Join => &[NodeId::IntegrateWinner],
            NodeId::IntegrateWinner => &[NodeId::FinalTest, NodeId::IntegrateWinner],
            NodeId::FinalTest => &[NodeId::FinalReview, NodeId::IntegrateWinner],
            NodeId::FinalReview => &[NodeId::CommitApproval, NodeId::IntegrateWinner],
            NodeId::CommitApproval => &[NodeId::Commit],
            NodeId::Commit => &[],
        }
    }

    pub fn accepts_successor(&self, next: NodeId) -> bool {
        self.allowed_successors().contains(&next)
    }

    pub fn validate_transition(&self, next: NodeId) -> Result<(), DomainError> {
        if self.accepts_successor(next) {
            Ok(())
        } else {
            Err(DomainError::IllegalNodeTransition {
                from: *self,
                to: next,
            })
        }
    }
}

impl fmt::Display for NodeId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for NodeId {
    type Err = DomainError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        NodeId::ALL
            .into_iter()
            .find(|node| node.as_str() == value)
            .ok_or_else(|| DomainError::UnknownNode {
                node: value.to_string(),
            })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum NodeScope {
    Run,
    Candidate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum NodeClass {
    Preparation,
    Agent,
    Command,
    Review,
    Decision,
    Git,
}

impl NodeId {
    pub fn class(&self) -> NodeClass {
        match self {
            NodeId::Prepare => NodeClass::Preparation,
            NodeId::Plan | NodeId::ImplementCandidate | NodeId::RepairCandidate => NodeClass::Agent,
            NodeId::TestCandidate | NodeId::FinalTest => NodeClass::Command,
            NodeId::ReviewCandidate | NodeId::FinalReview => NodeClass::Review,
            NodeId::Approval | NodeId::FanOut | NodeId::Join | NodeId::CommitApproval => NodeClass::Decision,
            NodeId::IntegrateWinner | NodeId::Commit => NodeClass::Git,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct GraphEdge {
    pub from: NodeId,
    pub to: NodeId,
    pub label: &'static str,
}

pub fn graph_edges() -> Vec<GraphEdge> {
    vec![
        GraphEdge { from: NodeId::Prepare, to: NodeId::Plan, label: "prepared" },
        GraphEdge { from: NodeId::Plan, to: NodeId::Approval, label: "plan written" },
        GraphEdge { from: NodeId::Approval, to: NodeId::FanOut, label: "approve" },
        GraphEdge { from: NodeId::Approval, to: NodeId::Plan, label: "revise" },
        GraphEdge { from: NodeId::FanOut, to: NodeId::ImplementCandidate, label: "candidate" },
        GraphEdge { from: NodeId::ImplementCandidate, to: NodeId::TestCandidate, label: "implemented" },
        GraphEdge { from: NodeId::TestCandidate, to: NodeId::ReviewCandidate, label: "tests passed" },
        GraphEdge { from: NodeId::TestCandidate, to: NodeId::RepairCandidate, label: "tests failed" },
        GraphEdge { from: NodeId::TestCandidate, to: NodeId::Join, label: "budget exhausted" },
        GraphEdge { from: NodeId::ReviewCandidate, to: NodeId::Join, label: "eligible" },
        GraphEdge { from: NodeId::ReviewCandidate, to: NodeId::RepairCandidate, label: "review failed" },
        GraphEdge { from: NodeId::RepairCandidate, to: NodeId::TestCandidate, label: "repaired" },
        GraphEdge { from: NodeId::RepairCandidate, to: NodeId::Join, label: "budget exhausted" },
        GraphEdge { from: NodeId::Join, to: NodeId::IntegrateWinner, label: "winner" },
        GraphEdge { from: NodeId::IntegrateWinner, to: NodeId::FinalTest, label: "applied" },
        GraphEdge { from: NodeId::IntegrateWinner, to: NodeId::IntegrateWinner, label: "promote next" },
        GraphEdge { from: NodeId::FinalTest, to: NodeId::FinalReview, label: "passed" },
        GraphEdge { from: NodeId::FinalTest, to: NodeId::IntegrateWinner, label: "promote next" },
        GraphEdge { from: NodeId::FinalReview, to: NodeId::CommitApproval, label: "passed" },
        GraphEdge { from: NodeId::FinalReview, to: NodeId::IntegrateWinner, label: "promote next" },
        GraphEdge { from: NodeId::CommitApproval, to: NodeId::Commit, label: "approved" },
    ]
}
