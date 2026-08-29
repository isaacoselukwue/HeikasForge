use std::path::PathBuf;
use std::sync::Arc;

use heikas_domain::event::EventPayload;
use heikas_domain::failure::NodeFailure;
use heikas_domain::graph::NodeId;
use heikas_domain::identity::{AttemptNumber, CandidateId, RunId};
use heikas_domain::node::{ArtifactReference, NodeStatus, StatePatch};
use heikas_domain::state::RunProjection;
use serde_json::Value;
use tokio::sync::watch;

use crate::configuration::EffectiveConfiguration;
use crate::engine::services::EngineServices;
use crate::model::attempt::AttemptEvidence;

#[derive(Clone)]
pub struct RunContext {
    pub run_id: RunId,
    pub repository: PathBuf,
    pub configuration: Arc<EffectiveConfiguration>,
    pub task_markdown: String,
    pub services: EngineServices,
    pub cancellation: watch::Receiver<bool>,
}

impl RunContext {
    pub fn task_title(&self) -> String {
        task_title_of(&self.task_markdown)
    }

    pub fn cancelled(&self) -> bool {
        *self.cancellation.borrow()
    }
}

pub fn task_title_of(markdown: &str) -> String {
    let first_line = markdown
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("Untitled task");
    let cleaned = first_line.trim_start_matches('#').trim();
    let mut title: String = cleaned.chars().take(120).collect();
    if title.is_empty() {
        title.push_str("Untitled task");
    }
    title
}

pub struct NodeContext<'a> {
    pub run: &'a RunContext,
    pub node: NodeId,
    pub candidate: Option<CandidateId>,
    pub attempt: AttemptNumber,
    pub projection: RunProjection,
}

impl NodeContext<'_> {
    pub fn services(&self) -> &EngineServices {
        &self.run.services
    }

    pub fn configuration(&self) -> &EffectiveConfiguration {
        &self.run.configuration
    }

    pub fn candidate_id(&self) -> Option<&CandidateId> {
        self.candidate.as_ref()
    }
}

pub struct NodeOutput {
    pub status: NodeStatus,
    pub next: Option<NodeId>,
    pub state_patch: StatePatch,
    pub artifacts: Vec<ArtifactReference>,
    pub failure: Option<NodeFailure>,
    pub metrics: Value,
    pub warnings: Vec<String>,
    pub events: Vec<EventPayload>,
    pub evidence: AttemptEvidence,
}

impl NodeOutput {
    pub fn succeeded(next: Option<NodeId>) -> Self {
        Self {
            status: NodeStatus::Succeeded,
            next,
            state_patch: StatePatch::default(),
            artifacts: Vec::new(),
            failure: None,
            metrics: Value::Object(serde_json::Map::new()),
            warnings: Vec::new(),
            events: Vec::new(),
            evidence: AttemptEvidence::default(),
        }
    }

    pub fn failed(failure: NodeFailure, next: Option<NodeId>) -> Self {
        Self {
            status: NodeStatus::Failed,
            next,
            state_patch: StatePatch::default(),
            artifacts: Vec::new(),
            failure: Some(failure),
            metrics: Value::Object(serde_json::Map::new()),
            warnings: Vec::new(),
            events: Vec::new(),
            evidence: AttemptEvidence::default(),
        }
    }

    pub fn paused() -> Self {
        Self {
            status: NodeStatus::Paused,
            next: None,
            state_patch: StatePatch::default(),
            artifacts: Vec::new(),
            failure: None,
            metrics: Value::Object(serde_json::Map::new()),
            warnings: Vec::new(),
            events: Vec::new(),
            evidence: AttemptEvidence::default(),
        }
    }

    pub fn with_patch(mut self, patch: StatePatch) -> Self {
        self.state_patch = patch;
        self
    }

    pub fn with_events(mut self, events: Vec<EventPayload>) -> Self {
        self.events = events;
        self
    }

    pub fn with_event(mut self, event: EventPayload) -> Self {
        self.events.push(event);
        self
    }

    pub fn with_artifacts(mut self, artifacts: Vec<ArtifactReference>) -> Self {
        self.artifacts = artifacts;
        self
    }

    pub fn with_evidence(mut self, evidence: AttemptEvidence) -> Self {
        self.evidence = evidence;
        self
    }

    pub fn with_metrics(mut self, metrics: Value) -> Self {
        self.metrics = metrics;
        self
    }

    pub fn with_warning(mut self, warning: impl Into<String>) -> Self {
        self.warnings.push(warning.into());
        self
    }
}
