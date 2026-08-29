use heikas_domain::graph::NodeId;
use heikas_domain::identity::{AttemptNumber, CandidateId, ContentDigest};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AttemptKey {
    pub node: NodeId,
    pub candidate: Option<CandidateId>,
    pub attempt: AttemptNumber,
}

impl AttemptKey {
    pub fn new(node: NodeId, candidate: Option<CandidateId>, attempt: AttemptNumber) -> Self {
        Self {
            node,
            candidate,
            attempt,
        }
    }

    pub fn directory_segments(&self) -> Vec<String> {
        match &self.candidate {
            Some(candidate) => vec![
                format!("{}-{}", self.node.as_str(), candidate),
                self.attempt.to_string(),
            ],
            None => vec![self.node.as_str().to_string(), self.attempt.to_string()],
        }
    }
}

#[derive(Debug, Clone)]
pub struct AttemptEvidence {
    pub input: serde_json::Value,
    pub invocation: Option<serde_json::Value>,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub stdout_truncated: bool,
    pub stderr_truncated: bool,
}

impl Default for AttemptEvidence {
    fn default() -> Self {
        Self {
            input: serde_json::Value::Object(serde_json::Map::new()),
            invocation: None,
            stdout: Vec::new(),
            stderr: Vec::new(),
            stdout_truncated: false,
            stderr_truncated: false,
        }
    }
}

impl AttemptEvidence {
    pub fn with_input(input: serde_json::Value) -> Self {
        Self {
            input,
            ..Self::default()
        }
    }

    pub fn with_streams(mut self, stdout: Vec<u8>, stderr: Vec<u8>) -> Self {
        self.stdout = stdout;
        self.stderr = stderr;
        self
    }

    pub fn with_invocation(mut self, invocation: serde_json::Value) -> Self {
        self.invocation = Some(invocation);
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct StoredArtifact {
    pub id: ContentDigest,
    pub label: String,
    pub relative_path: String,
    pub media_type: String,
    pub byte_length: u64,
    pub truncated: bool,
}

impl StoredArtifact {
    pub fn to_reference(&self) -> heikas_domain::node::ArtifactReference {
        heikas_domain::node::ArtifactReference {
            id: self.id.clone(),
            label: self.label.clone(),
            relative_path: self.relative_path.clone(),
            media_type: self.media_type.clone(),
            byte_length: self.byte_length,
            truncated: self.truncated,
        }
    }
}
