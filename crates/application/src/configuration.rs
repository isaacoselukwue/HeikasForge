use std::path::PathBuf;

use heikas_domain::budget::{CandidateCount, QualityProfile, RunBudgets};
use heikas_domain::clock::{TimeoutSeconds, Timestamp};
use heikas_domain::command::{
    CommandCatalogue, CommandKind, CommandSpecification, ReportFormat,
    MAXIMUM_COMMAND_TIMEOUT_SECONDS,
};
use heikas_domain::identity::ContentDigest;
use heikas_domain::path_policy::PathPolicy;
use heikas_domain::retry::{NodeTimeouts, RetryPolicy};
use heikas_domain::run::CommitPolicy;
use serde::{Deserialize, Serialize};

use crate::error::{ApplicationError, ApplicationResult};

pub const CONFIGURATION_SCHEMA_VERSION: u32 = 1;
pub const REPOSITORY_CONFIGURATION_RELATIVE_PATH: &str = ".heikas/forge.toml";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ConfigurationAuthority {
    UserConfiguration,
    Repository,
}

impl ConfigurationAuthority {
    pub fn as_str(&self) -> &'static str {
        match self {
            ConfigurationAuthority::UserConfiguration => "user_configuration",
            ConfigurationAuthority::Repository => "repository",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RepositoryTrustState {
    NoRepositoryConfiguration,
    Trusted,
    Untrusted,
}

impl RepositoryTrustState {
    pub fn as_str(&self) -> &'static str {
        match self {
            RepositoryTrustState::NoRepositoryConfiguration => "no_repository_configuration",
            RepositoryTrustState::Trusted => "trusted",
            RepositoryTrustState::Untrusted => "untrusted",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            RepositoryTrustState::NoRepositoryConfiguration => {
                "the repository declares no configuration"
            }
            RepositoryTrustState::Trusted => "the repository configuration is trusted",
            RepositoryTrustState::Untrusted => "the repository configuration is not trusted",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum WithheldReason {
    UserConfigurationOnly,
    RequiresRepositoryTrust,
    WouldWeakenPolicy,
}

impl WithheldReason {
    pub fn as_str(&self) -> &'static str {
        match self {
            WithheldReason::UserConfigurationOnly => "user_configuration_only",
            WithheldReason::RequiresRepositoryTrust => "requires_repository_trust",
            WithheldReason::WouldWeakenPolicy => "would_weaken_policy",
        }
    }

    pub fn explanation(&self) -> &'static str {
        match self {
            WithheldReason::UserConfigurationOnly => {
                "only your own user configuration may set this, because a repository could otherwise redirect your credentials or authorship"
            }
            WithheldReason::RequiresRepositoryTrust => {
                "this names an executable or its arguments, so it is honoured only after you trust the repository configuration"
            }
            WithheldReason::WouldWeakenPolicy => {
                "a repository may tighten a safety setting but never relax one"
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct WithheldSetting {
    pub setting: String,
    pub reason: WithheldReason,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepositoryTrustDecision {
    pub state: RepositoryTrustState,
    pub configuration_digest: Option<ContentDigest>,
    pub withheld: Vec<WithheldSetting>,
}

impl Default for RepositoryTrustDecision {
    fn default() -> Self {
        Self {
            state: RepositoryTrustState::NoRepositoryConfiguration,
            configuration_digest: None,
            withheld: Vec::new(),
        }
    }
}

impl RepositoryTrustDecision {
    pub fn honoured_in_full(&self) -> bool {
        self.withheld.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case", tag = "kind", content = "detail")]
pub enum CommandCatalogueSource {
    UserConfiguration,
    RepositoryConfiguration,
    Detected(String),
    NothingDetected(Vec<String>),
    DeclaredForThisRun,
}

impl Default for CommandCatalogueSource {
    fn default() -> Self {
        CommandCatalogueSource::NothingDetected(Vec::new())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepositoryTrustRecord {
    pub repository_path: String,
    pub configuration_digest: ContentDigest,
    pub granted_at: Timestamp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum NetworkPolicy {
    Disabled,
    LoopbackOnly,
    ApprovedEndpoints,
}

impl NetworkPolicy {
    pub fn permissiveness_rank(&self) -> u8 {
        match self {
            NetworkPolicy::Disabled => 0,
            NetworkPolicy::LoopbackOnly => 1,
            NetworkPolicy::ApprovedEndpoints => 2,
        }
    }

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
    #[serde(default)]
    pub repository_trust: RepositoryTrustDecision,
    #[serde(default)]
    pub command_source: CommandCatalogueSource,
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
        let mut missing: Vec<CommandKind> = Vec::new();
        if self.commands.of_kind(CommandKind::Test).is_empty() {
            missing.push(CommandKind::Test);
        }
        for kind in self.required_review_kinds() {
            if self.commands.of_kind(kind).is_empty() {
                missing.push(kind);
            }
        }
        if !missing.is_empty() {
            return Err(ApplicationError::InvalidConfiguration(
                self.missing_commands_detail(&missing),
            ));
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
                "the deterministic demonstration agent may only run in demonstration mode"
                    .to_string(),
            ));
        }
        Ok(())
    }

    pub fn missing_commands_detail(&self, missing: &[CommandKind]) -> String {
        let kinds = missing
            .iter()
            .map(|kind| format!("`{}`", kind.as_str()))
            .collect::<Vec<_>>()
            .join(", ");
        let declarations = missing
            .iter()
            .map(|kind| format!("--command {}=<program>", kind.as_str()))
            .collect::<Vec<_>>()
            .join(" ");
        format!(
            "the {} quality profile needs a command of kind {kinds}. {} Declare them for this run with `{declarations}`, giving each argument separately as `--command-arg <kind>=<argument>`, or write `[[commands]]` entries into `{REPOSITORY_CONFIGURATION_RELATIVE_PATH}` and run `heikas trust {}`.",
            self.quality.profile.as_str(),
            self.command_source_detail(),
            self.repository_path.display()
        )
    }

    fn command_source_detail(&self) -> String {
        let repository = self.repository_path.display();
        let commands_were_withheld = self
            .repository_trust
            .withheld
            .iter()
            .any(|entry| entry.setting == "commands");
        if commands_were_withheld {
            return format!(
                "`{REPOSITORY_CONFIGURATION_RELATIVE_PATH}` declares commands that have not been trusted, so none of them was honoured."
            );
        }
        match &self.command_source {
            CommandCatalogueSource::NothingDetected(markers) => format!(
                "No project was recognised in `{repository}`, because none of {} is present.",
                describe_markers(markers)
            ),
            CommandCatalogueSource::Detected(kind) => format!(
                "`{repository}` was detected as a {kind} project, which proposed {} commands.",
                self.commands.commands.len()
            ),
            CommandCatalogueSource::DeclaredForThisRun => format!(
                "{} commands were declared for this run.",
                self.commands.commands.len()
            ),
            CommandCatalogueSource::UserConfiguration => format!(
                "Your user configuration declares {} commands.",
                self.commands.commands.len()
            ),
            CommandCatalogueSource::RepositoryConfiguration => format!(
                "`{REPOSITORY_CONFIGURATION_RELATIVE_PATH}` declares {} commands.",
                self.commands.commands.len()
            ),
        }
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

fn describe_markers(markers: &[String]) -> String {
    if markers.is_empty() {
        return "any recognised project manifest".to_string();
    }
    match markers.split_last() {
        Some((last, [])) => format!("`{last}`"),
        Some((last, rest)) => format!(
            "{} and `{last}`",
            rest.iter()
                .map(|marker| format!("`{marker}`"))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        None => "any recognised project manifest".to_string(),
    }
}
