use std::path::PathBuf;

use heikas_application::configuration::NetworkPolicy;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ForgeDocument {
    #[serde(default)]
    pub schema_version: Option<u32>,
    #[serde(default)]
    pub run: Option<RunSection>,
    #[serde(default)]
    pub agent: Option<AgentSection>,
    #[serde(default)]
    pub quality: Option<QualitySection>,
    #[serde(default)]
    pub git: Option<GitSection>,
    #[serde(default)]
    pub policy: Option<PolicySection>,
    #[serde(default)]
    pub redaction: Option<RedactionSection>,
    #[serde(default)]
    pub environment: Option<EnvironmentSection>,
    #[serde(default)]
    pub commands: Option<Vec<CommandSection>>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunSection {
    pub candidates: Option<u8>,
    pub max_parallel_candidates: Option<u8>,
    pub max_repairs_per_candidate: Option<u32>,
    pub commit_policy: Option<String>,
    pub require_clean_repository: Option<bool>,
    pub wall_clock_seconds: Option<u32>,
    pub max_output_bytes_per_stream: Option<u64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentSection {
    pub driver: Option<String>,
    pub model: Option<String>,
    pub endpoint: Option<String>,
    pub api_key_environment_variable: Option<String>,
    pub executable: Option<String>,
    pub extra_arguments: Option<Vec<String>>,
    pub max_turns: Option<u32>,
    pub timeout_seconds: Option<u32>,
    pub network: Option<NetworkPolicy>,
    pub fixture_script: Option<PathBuf>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QualitySection {
    pub profile: Option<String>,
    pub minimum_line_coverage: Option<f64>,
    pub protect_existing_tests: Option<bool>,
    pub sonar_scanner: Option<SonarScannerSection>,
    pub sonar_mcp: Option<SonarMcpSection>,
    pub ai_review: Option<AiReviewSection>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SonarScannerSection {
    pub enabled: Option<bool>,
    pub program: Option<String>,
    pub arguments: Option<Vec<String>>,
    pub host_url: Option<String>,
    pub project_key: Option<String>,
    pub token_environment_variable: Option<String>,
    pub wait_for_quality_gate: Option<bool>,
    pub timeout_seconds: Option<u32>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SonarMcpSection {
    pub enabled: Option<bool>,
    pub program: Option<String>,
    pub arguments: Option<Vec<String>>,
    pub token_environment_variable: Option<String>,
    pub project_key: Option<String>,
    pub timeout_seconds: Option<u32>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AiReviewSection {
    pub enabled: Option<bool>,
    pub advisory_only: Option<bool>,
    pub gate_rules: Option<Vec<String>>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GitSection {
    pub branch_prefix: Option<String>,
    pub author_name: Option<String>,
    pub include_dirty: Option<bool>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PolicySection {
    pub protected_paths: Option<Vec<String>>,
    pub sensitive_paths: Option<Vec<String>>,
    pub maximum_read_bytes: Option<u64>,
    pub maximum_write_bytes: Option<u64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RedactionSection {
    pub secret_environment_variables: Option<Vec<String>>,
    pub additional_patterns: Option<Vec<String>>,
    pub redact_home_prefix: Option<bool>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnvironmentSection {
    pub allowlist: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommandSection {
    pub id: String,
    pub program: String,
    #[serde(default)]
    pub args: Vec<String>,
    pub kind: String,
    #[serde(default)]
    pub timeout_seconds: Option<u32>,
    #[serde(default)]
    pub required: Option<bool>,
    #[serde(default)]
    pub report_format: Option<String>,
    #[serde(default)]
    pub report_path: Option<String>,
    #[serde(default)]
    pub working_subdirectory: Option<String>,
    #[serde(default)]
    pub success_exit_codes: Option<Vec<i32>>,
    #[serde(default)]
    pub environment: Option<Vec<(String, String)>>,
}
