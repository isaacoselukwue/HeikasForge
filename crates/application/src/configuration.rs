use std::path::PathBuf;

use heikas_domain::budget::{CandidateCount, QualityProfile, RunBudgets};
use heikas_domain::clock::TimeoutSeconds;
use heikas_domain::command::{CommandCatalogue, CommandKind, CommandSpecification, ReportFormat, MAXIMUM_COMMAND_TIMEOUT_SECONDS};
use heikas_domain::identity::ContentDigest;
use heikas_domain::path_policy::PathPolicy;
use heikas_domain::retry::{NodeTimeouts, RetryPolicy};
use heikas_domain::run::CommitPolicy;
use serde::{Deserialize, Serialize};

use crate::error::{ApplicationError, ApplicationResult};

pub const CONFIGURATION_SCHEMA_VERSION: u32 = 1;
pub const REPOSITORY_CONFIGURATION_RELATIVE_PATH: &str = ".heikas/forge.toml";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum NetworkPolicy {
    Disabled,
    LoopbackOnly,
    ApprovedEndpoints,
}

impl NetworkPolicy {
    pub fn as_str(&self) -> &'static str {
        match self {
            NetworkPolicy::Disabled => "disabled",
            NetworkPolicy::LoopbackOnly => "loopback-only",
            NetworkPolicy::ApprovedEndpoints => "approved-endpoints",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AgentDriverKind {
    Local,
    Fake,
    ClaudeCode,
    CodexCli,
    OpenCode,
    GenericProcess,
}

impl AgentDriverKind {
    pub const ALL: [AgentDriverKind; 6] = [
        AgentDriverKind::Local,
        AgentDriverKind::Fake,
        AgentDriverKind::ClaudeCode,
        AgentDriverKind::CodexCli,
        AgentDriverKind::OpenCode,
        AgentDriverKind::GenericProcess,
    ];

    pub fn as_str(&self) -> &'static str {
        match self {
            AgentDriverKind::Local => "local",
            AgentDriverKind::Fake => "fake",
            AgentDriverKind::ClaudeCode => "claude-code",
            AgentDriverKind::CodexCli => "codex-cli",
            AgentDriverKind::OpenCode => "opencode",
            AgentDriverKind::GenericProcess => "generic-process",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            AgentDriverKind::Local => "Built-in local tool agent",
            AgentDriverKind::Fake => "Deterministic demonstration agent",
            AgentDriverKind::ClaudeCode => "External Claude Code CLI",
            AgentDriverKind::CodexCli => "External Codex CLI",
            AgentDriverKind::OpenCode => "External OpenCode CLI",
            AgentDriverKind::GenericProcess => "External generic process adapter",
        }
    }

    pub fn requires_paid_account(&self) -> bool {
        matches!(
            self,
            AgentDriverKind::ClaudeCode | AgentDriverKind::CodexCli | AgentDriverKind::OpenCode
        )
    }

    pub fn is_demonstration_only(&self) -> bool {
        matches!(self, AgentDriverKind::Fake)
    }
}

impl std::str::FromStr for AgentDriverKind {
    type Err = ApplicationError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        AgentDriverKind::ALL
            .into_iter()
            .find(|kind| kind.as_str() == value)
            .ok_or_else(|| {
                ApplicationError::InvalidConfiguration(format!("unknown agent driver `{value}`"))
            })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AgentConfiguration {
    pub driver: AgentDriverKind,
    pub model: Option<String>,
    pub endpoint: Option<String>,
    pub api_key_environment_variable: Option<String>,
    pub executable: Option<String>,
    pub extra_arguments: Vec<String>,
    pub max_turns: u32,
    pub timeout: TimeoutSeconds,
    pub network: NetworkPolicy,
    pub fixture_script: Option<PathBuf>,
}

impl Default for AgentConfiguration {
    fn default() -> Self {
        Self {
            driver: AgentDriverKind::Local,
            model: None,
            endpoint: Some("http://127.0.0.1:11434/v1".to_string()),
            api_key_environment_variable: None,
            executable: None,
            extra_arguments: Vec::new(),
            max_turns: 40,
            timeout: TimeoutSeconds::clamped(1_200, MAXIMUM_COMMAND_TIMEOUT_SECONDS),
            network: NetworkPolicy::LoopbackOnly,
            fixture_script: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SonarScannerConfiguration {
    pub enabled: bool,
    pub program: String,
    pub arguments: Vec<String>,
    pub host_url: String,
    pub project_key: Option<String>,
    pub token_environment_variable: Option<String>,
    pub wait_for_quality_gate: bool,
    pub timeout: TimeoutSeconds,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SonarMcpConfiguration {
    pub enabled: bool,
    pub program: String,
    pub arguments: Vec<String>,
    pub token_environment_variable: Option<String>,
    pub project_key: Option<String>,
    pub timeout: TimeoutSeconds,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AiReviewConfiguration {
    pub enabled: bool,
    pub advisory_only: bool,
    pub gate_rules: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct QualityConfiguration {
    pub profile: QualityProfile,
    pub minimum_line_coverage: Option<f64>,
    pub protect_existing_tests: bool,
    pub sonar_scanner: SonarScannerConfiguration,
    pub sonar_mcp: SonarMcpConfiguration,
    pub ai_review: AiReviewConfiguration,
}

impl Default for QualityConfiguration {
    fn default() -> Self {
        Self {
            profile: QualityProfile::Strict,
            minimum_line_coverage: None,
            protect_existing_tests: true,
            sonar_scanner: SonarScannerConfiguration {
                enabled: false,
                program: "sonar-scanner".to_string(),
                arguments: Vec::new(),
                host_url: "http://127.0.0.1:9000".to_string(),
                project_key: None,
                token_environment_variable: Some("SONAR_TOKEN".to_string()),
                wait_for_quality_gate: true,
                timeout: TimeoutSeconds::clamped(600, MAXIMUM_COMMAND_TIMEOUT_SECONDS),
            },
            sonar_mcp: SonarMcpConfiguration {
                enabled: false,
                program: "sonarqube-mcp-server".to_string(),
                arguments: Vec::new(),
                token_environment_variable: Some("SONAR_TOKEN".to_string()),
                project_key: None,
                timeout: TimeoutSeconds::clamped(600, MAXIMUM_COMMAND_TIMEOUT_SECONDS),
            },
            ai_review: AiReviewConfiguration {
                enabled: false,
                advisory_only: true,
                gate_rules: Vec::new(),
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct GitConfiguration {
    pub branch_prefix: String,
    pub author_name: String,
    pub include_dirty: bool,
    pub require_clean_repository: bool,
}

impl Default for GitConfiguration {
    fn default() -> Self {
        Self {
            branch_prefix: "heikas/run-".to_string(),
            author_name: "Isaac Oselukwue".to_string(),
            include_dirty: false,
            require_clean_repository: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RedactionConfiguration {
    pub secret_environment_variables: Vec<String>,
    pub additional_patterns: Vec<String>,
    pub redact_home_prefix: bool,
}

impl Default for RedactionConfiguration {
    fn default() -> Self {
        Self {
            secret_environment_variables: vec![
                "GITHUB_TOKEN".to_string(),
                "GH_TOKEN".to_string(),
                "SONAR_TOKEN".to_string(),
                "ANTHROPIC_API_KEY".to_string(),
                "OPENAI_API_KEY".to_string(),
                "AWS_SECRET_ACCESS_KEY".to_string(),
                "AWS_SESSION_TOKEN".to_string(),
                "NPM_TOKEN".to_string(),
                "CARGO_REGISTRY_TOKEN".to_string(),
            ],
            additional_patterns: Vec::new(),
            redact_home_prefix: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct EffectiveConfiguration {
    pub schema_version: u32,
    pub repository_path: PathBuf,
    pub budgets: RunBudgets,
    pub commit_policy: CommitPolicy,
    pub agent: AgentConfiguration,
    pub quality: QualityConfiguration,
    pub git: GitConfiguration,
    pub commands: CommandCatalogue,
    pub path_policy: PathPolicy,
    pub redaction: RedactionConfiguration,
    pub retry: RetryPolicy,
    pub timeouts: NodeTimeouts,
    pub environment_allowlist: Vec<String>,
    pub demonstration_mode: bool,
}

impl EffectiveConfiguration {
    pub fn digest(&self) -> ApplicationResult<ContentDigest> {
        let encoded = serde_json::to_vec(self)?;
        Ok(ContentDigest::of_bytes(&encoded))
    }

    pub fn validate(&self) -> ApplicationResult<()> {
        self.budgets.validate()?;
        self.commands.validate()?;
        if self.git.author_name.trim().is_empty() {
            return Err(ApplicationError::InvalidConfiguration(
                "the Git author name must not be empty".to_string(),
            ));
        }
        if self.git.branch_prefix.trim().is_empty() {
            return Err(ApplicationError::InvalidConfiguration(
                "the Git branch prefix must not be empty".to_string(),
            ));
        }
        if self.commands.of_kind(CommandKind::Test).is_empty() {
            return Err(ApplicationError::InvalidConfiguration(
                "at least one test command must be configured before a run can start".to_string(),
            ));
        }
        let required_kinds = self.required_review_kinds();
        for kind in required_kinds {
            if self.commands.of_kind(kind).is_empty() {
                return Err(ApplicationError::InvalidConfiguration(format!(
                    "the {} quality profile requires a `{}` command",
                    self.quality.profile.as_str(),
                    kind.as_str()
                )));
            }
        }
        for command in &self.commands.commands {
            if command.required
                && command.report_format != ReportFormat::None
                && command.report_path.is_none()
            {
                return Err(ApplicationError::InvalidConfiguration(format!(
                    "command `{}` declares a report format without a report path",
                    command.id
                )));
            }
        }
        if self.agent.driver.is_demonstration_only() && !self.demonstration_mode {
            return Err(ApplicationError::InvalidConfiguration(
                "the deterministic demonstration agent may only run in demonstration mode".to_string(),
            ));
        }
        Ok(())
    }

    pub fn required_review_kinds(&self) -> Vec<CommandKind> {
        match self.quality.profile {
            QualityProfile::Standard => vec![CommandKind::Lint],
            QualityProfile::Strict => vec![
                CommandKind::Format,
                CommandKind::Lint,
                CommandKind::Audit,
                CommandKind::SecretScan,
                CommandKind::StaticAnalysis,
                CommandKind::Policy,
            ],
        }
    }

    pub fn minimum_line_coverage(&self) -> Option<f64> {
        self.quality
            .minimum_line_coverage
            .or_else(|| self.quality.profile.default_minimum_line_coverage())
    }

    pub fn required_commands(&self) -> Vec<&CommandSpecification> {
        self.commands
            .commands
            .iter()
            .filter(|command| command.required)
            .collect()
    }

    pub fn candidate_count(&self) -> CandidateCount {
        self.budgets.candidates
    }

    pub fn review_provider_names(&self) -> Vec<String> {
        let mut providers = vec!["local".to_string()];
        if self.quality.sonar_scanner.enabled {
            providers.push("sonar-scanner".to_string());
        }
        if self.quality.sonar_mcp.enabled {
            providers.push("sonar-mcp".to_string());
        }
        if self.quality.ai_review.enabled {
            providers.push("ai-review".to_string());
        }
        providers
    }
}
