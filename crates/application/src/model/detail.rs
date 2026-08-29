use heikas_domain::event::DurableEvent;
use heikas_domain::graph::GraphEdge;
use heikas_domain::identity::RunId;
use heikas_domain::state::{RunMetrics, RunProjection};
use serde::{Deserialize, Serialize};

use crate::model::run_summary::{CandidateView, RunSummary, TimelineEntry};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct GraphView {
    pub nodes: Vec<GraphNodeView>,
    pub edges: Vec<GraphEdgeView>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct GraphNodeView {
    pub id: String,
    pub label: String,
    pub scope: String,
    pub class: String,
    pub state: GraphNodeState,
    pub attempts: u32,
    pub total_duration_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum GraphNodeState {
    Pending,
    Active,
    Succeeded,
    Failed,
    Paused,
    Skipped,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct GraphEdgeView {
    pub from: String,
    pub to: String,
    pub label: String,
    pub traversed: bool,
}

impl GraphEdgeView {
    pub fn from_edge(edge: &GraphEdge, traversed: bool) -> Self {
        Self {
            from: edge.from.as_str().to_string(),
            to: edge.to.as_str().to_string(),
            label: edge.label.to_string(),
            traversed,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RunDetail {
    pub summary: RunSummary,
    pub projection: RunProjection,
    pub candidates: Vec<CandidateView>,
    pub graph: GraphView,
    pub timeline: Vec<TimelineEntry>,
    pub metrics: RunMetrics,
    pub ranking_rationale: Vec<String>,
    pub integration_detail: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct EventPage {
    pub run_id: RunId,
    pub events: Vec<DurableEvent>,
    pub next_sequence: u64,
    pub complete: bool,
}
