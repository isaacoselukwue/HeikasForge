use std::path::{Path, PathBuf};
use std::str::FromStr;

use async_trait::async_trait;
use heikas_application::configuration::{
    AgentConfiguration, AgentDriverKind, AiReviewConfiguration, EffectiveConfiguration,
    GitConfiguration, QualityConfiguration, RedactionConfiguration, SonarMcpConfiguration,
    SonarScannerConfiguration, CONFIGURATION_SCHEMA_VERSION,
    REPOSITORY_CONFIGURATION_RELATIVE_PATH,
};
use heikas_application::error::{ApplicationError, ApplicationResult};
use heikas_application::model::request::CreateRunRequest;
use heikas_application::ports::runtime::ConfigurationResolver;
use heikas_domain::budget::{CandidateCount, QualityProfile, RunBudgets};
use heikas_domain::clock::TimeoutSeconds;
use heikas_domain::command::{
    CommandCatalogue, CommandId, CommandKind, CommandSpecification, ReportFormat,
    MAXIMUM_COMMAND_TIMEOUT_SECONDS,
};
use heikas_domain::path_policy::PathPolicy;
use heikas_domain::retry::{NodeTimeouts, RetryPolicy};
use heikas_domain::run::CommitPolicy;

use crate::atomic::write_atomic;
use crate::configuration::detection::{detect_project_kind, proposed_commands};
use crate::configuration::document::{CommandSection, ForgeDocument};
use crate::layout::StoreLayout;
use crate::process::supervisor::essential_environment_variables;

pub struct LayeredConfigurationResolver {
    layout: StoreLayout,
}

impl LayeredConfigurationResolver {
    pub fn new(layout: StoreLayout) -> Self {
        Self { layout }
    }

    fn base_configuration(repository: &Path) -> EffectiveConfiguration {
        EffectiveConfiguration {
            schema_version: CONFIGURATION_SCHEMA_VERSION,
            repository_path: repository.to_path_buf(),
            budgets: RunBudgets::default(),
            commit_policy: CommitPolicy::Manual,
            agent: AgentConfiguration::default(),
            quality: QualityConfiguration {
                profile: QualityProfile::Standard,
                ..QualityConfiguration::default()
            },
            git: GitConfiguration::default(),
            commands: CommandCatalogue::default(),
            path_policy: PathPolicy::default(),
            redaction: RedactionConfiguration::default(),
            retry: RetryPolicy::default(),
            timeouts: NodeTimeouts::default(),
            environment_allowlist: essential_environment_variables()
                .into_iter()
                .map(str::to_string)
                .collect(),
            demonstration_mode: false,
        }
    }

    fn read_document(path: &Path) -> ApplicationResult<Option<ForgeDocument>> {
        let contents = match std::fs::read_to_string(path) {
            Ok(contents) => contents,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(crate::atomic::storage(path, "read", error)),
        };
        let document: ForgeDocument = toml::from_str(&contents).map_err(|error| {
            ApplicationError::InvalidConfiguration(format!(
                "`{}` could not be parsed: {error}",
                path.display()
            ))
        })?;
        if let Some(version) = document.schema_version {
            if version != CONFIGURATION_SCHEMA_VERSION {
                return Err(ApplicationError::InvalidConfiguration(format!(
                    "`{}` declares schema version {version} but {CONFIGURATION_SCHEMA_VERSION} is required",
                    path.display()
                )));
            }
        }
        Ok(Some(document))
    }

    fn apply(
        configuration: &mut EffectiveConfiguration,
        document: &ForgeDocument,
    ) -> ApplicationResult<()> {
        if let Some(run) = &document.run {
            if let Some(candidates) = run.candidates {
                configuration.budgets.candidates = CandidateCount::new(candidates)?;
            }
            if let Some(parallel) = run.max_parallel_candidates {
                configuration.budgets.max_parallel_candidates = parallel;
            }
            if let Some(repairs) = run.max_repairs_per_candidate {
                configuration.budgets.max_repairs_per_candidate = repairs;
            }
            if let Some(seconds) = run.wall_clock_seconds {
                configuration.budgets.wall_clock_seconds = seconds;
            }
            if let Some(bytes) = run.max_output_bytes_per_stream {
                configuration.budgets.max_output_bytes_per_stream = bytes;
            }
            if let Some(policy) = &run.commit_policy {
                configuration.commit_policy = CommitPolicy::from_str(policy)?;
            }
            if let Some(require_clean) = run.require_clean_repository {
                configuration.git.require_clean_repository = require_clean;
            }
        }
        if let Some(agent) = &document.agent {
            if let Some(driver) = &agent.driver {
                configuration.agent.driver = AgentDriverKind::from_str(driver)?;
            }
            if agent.model.is_some() {
                configuration.agent.model = agent.model.clone();
            }
            if agent.endpoint.is_some() {
                configuration.agent.endpoint = agent.endpoint.clone();
            }
            if agent.api_key_environment_variable.is_some() {
                configuration.agent.api_key_environment_variable =
                    agent.api_key_environment_variable.clone();
            }
            if agent.executable.is_some() {
                configuration.agent.executable = agent.executable.clone();
            }
            if let Some(extra) = &agent.extra_arguments {
                configuration.agent.extra_arguments = extra.clone();
            }
            if let Some(turns) = agent.max_turns {
                configuration.agent.max_turns = turns;
                configuration.budgets.max_agent_turns = turns;
            }
            if let Some(seconds) = agent.timeout_seconds {
                configuration.agent.timeout =
                    TimeoutSeconds::clamped(seconds, MAXIMUM_COMMAND_TIMEOUT_SECONDS);
                configuration.timeouts.agent_seconds = seconds;
            }
            if let Some(network) = agent.network {
                configuration.agent.network = network;
            }
            if agent.fixture_script.is_some() {
                configuration.agent.fixture_script = agent.fixture_script.clone();
            }
        }
        if let Some(quality) = &document.quality {
            if let Some(profile) = &quality.profile {
                configuration.quality.profile = QualityProfile::from_str(profile)?;
            }
            if quality.minimum_line_coverage.is_some() {
                configuration.quality.minimum_line_coverage = quality.minimum_line_coverage;
            }
            if let Some(protect) = quality.protect_existing_tests {
                configuration.quality.protect_existing_tests = protect;
            }
            if let Some(scanner) = &quality.sonar_scanner {
                let target = &mut configuration.quality.sonar_scanner;
                apply_scanner(target, scanner);
            }
            if let Some(mcp) = &quality.sonar_mcp {
                let target = &mut configuration.quality.sonar_mcp;
                apply_mcp(target, mcp);
            }
            if let Some(ai) = &quality.ai_review {
                let target = &mut configuration.quality.ai_review;
                apply_ai(target, ai);
            }
        }
        if let Some(git) = &document.git {
            if let Some(prefix) = &git.branch_prefix {
                configuration.git.branch_prefix = prefix.clone();
            }
            if let Some(author) = &git.author_name {
                configuration.git.author_name = author.clone();
            }
            if let Some(include_dirty) = git.include_dirty {
                configuration.git.include_dirty = include_dirty;
            }
        }
        if let Some(policy) = &document.policy {
            if let Some(protected) = &policy.protected_paths {
                configuration.path_policy.protected_patterns = protected.clone();
            }
            if let Some(sensitive) = &policy.sensitive_paths {
                configuration.path_policy.sensitive_patterns = sensitive.clone();
            }
            if let Some(bytes) = policy.maximum_read_bytes {
                configuration.path_policy.maximum_read_bytes = bytes;
            }
            if let Some(bytes) = policy.maximum_write_bytes {
                configuration.path_policy.maximum_write_bytes = bytes;
            }
        }
        if let Some(redaction) = &document.redaction {
            if let Some(variables) = &redaction.secret_environment_variables {
                configuration.redaction.secret_environment_variables = variables.clone();
            }
            if let Some(patterns) = &redaction.additional_patterns {
                configuration.redaction.additional_patterns = patterns.clone();
            }
            if let Some(redact_home) = redaction.redact_home_prefix {
                configuration.redaction.redact_home_prefix = redact_home;
            }
        }
        if let Some(environment) = &document.environment {
            if let Some(allowlist) = &environment.allowlist {
                let mut merged = essential_environment_variables()
                    .into_iter()
                    .map(str::to_string)
                    .collect::<Vec<_>>();
                for name in allowlist {
                    if !merged.contains(name) {
                        merged.push(name.clone());
                    }
                }
                configuration.environment_allowlist = merged;
            }
        }
        if let Some(commands) = &document.commands {
            let mut catalogue = CommandCatalogue::default();
            for section in commands {
                catalogue.commands.push(convert_command(section)?);
            }
            configuration.commands = catalogue;
        }
        Ok(())
    }
}

fn apply_scanner(
    target: &mut SonarScannerConfiguration,
    section: &crate::configuration::document::SonarScannerSection,
) {
    if let Some(enabled) = section.enabled {
        target.enabled = enabled;
    }
    if let Some(program) = &section.program {
        target.program = program.clone();
    }
    if let Some(arguments) = &section.arguments {
        target.arguments = arguments.clone();
    }
    if let Some(host) = &section.host_url {
        target.host_url = host.clone();
    }
    if section.project_key.is_some() {
        target.project_key = section.project_key.clone();
    }
    if section.token_environment_variable.is_some() {
        target.token_environment_variable = section.token_environment_variable.clone();
    }
    if let Some(wait) = section.wait_for_quality_gate {
        target.wait_for_quality_gate = wait;
    }
    if let Some(seconds) = section.timeout_seconds {
        target.timeout = TimeoutSeconds::clamped(seconds, MAXIMUM_COMMAND_TIMEOUT_SECONDS);
    }
}

fn apply_mcp(
    target: &mut SonarMcpConfiguration,
    section: &crate::configuration::document::SonarMcpSection,
) {
    if let Some(enabled) = section.enabled {
        target.enabled = enabled;
    }
    if let Some(program) = &section.program {
        target.program = program.clone();
    }
    if let Some(arguments) = &section.arguments {
        target.arguments = arguments.clone();
    }
    if section.token_environment_variable.is_some() {
        target.token_environment_variable = section.token_environment_variable.clone();
    }
    if section.project_key.is_some() {
        target.project_key = section.project_key.clone();
    }
    if let Some(seconds) = section.timeout_seconds {
        target.timeout = TimeoutSeconds::clamped(seconds, MAXIMUM_COMMAND_TIMEOUT_SECONDS);
    }
}

fn apply_ai(
    target: &mut AiReviewConfiguration,
    section: &crate::configuration::document::AiReviewSection,
) {
    if let Some(enabled) = section.enabled {
        target.enabled = enabled;
    }
    if let Some(advisory) = section.advisory_only {
        target.advisory_only = advisory;
    }
    if let Some(rules) = &section.gate_rules {
        target.gate_rules = rules.clone();
    }
}

fn convert_command(section: &CommandSection) -> ApplicationResult<CommandSpecification> {
    let specification = CommandSpecification {
        id: CommandId::from_str(&section.id)?,
        kind: CommandKind::from_str(&section.kind)?,
        program: section.program.clone(),
        args: section.args.clone(),
        working_subdirectory: section.working_subdirectory.clone(),
        timeout: TimeoutSeconds::clamped(
            section.timeout_seconds.unwrap_or(600),
            MAXIMUM_COMMAND_TIMEOUT_SECONDS,
        ),
        required: section.required.unwrap_or(true),
        report_format: section
            .report_format
            .as_deref()
            .map(ReportFormat::from_str)
            .transpose()?
            .unwrap_or(ReportFormat::None),
        report_path: section.report_path.clone(),
        environment: section.environment.clone().unwrap_or_default(),
        success_exit_codes: section.success_exit_codes.clone().unwrap_or_else(|| vec![0]),
    };
    specification.validate()?;
    Ok(specification)
}

#[async_trait]
impl ConfigurationResolver for LayeredConfigurationResolver {
    async fn detect(&self, repository: &Path) -> ApplicationResult<EffectiveConfiguration> {
        let mut configuration = Self::base_configuration(repository);
        if let Some(document) = Self::read_document(&self.layout.user_configuration())? {
            Self::apply(&mut configuration, &document)?;
        }
        let repository_configuration = repository.join(REPOSITORY_CONFIGURATION_RELATIVE_PATH);
        match Self::read_document(&repository_configuration)? {
            Some(document) => Self::apply(&mut configuration, &document)?,
            None => {
                let kind = detect_project_kind(repository);
                configuration.commands = CommandCatalogue {
                    commands: proposed_commands(kind),
                };
            }
        }
        Ok(configuration)
    }

    async fn resolve(&self, request: &CreateRunRequest) -> ApplicationResult<EffectiveConfiguration> {
        let repository = crate::paths::canonical_root(&request.repository_path)?;
        let mut configuration = self.detect(&repository).await?;
        configuration.repository_path = repository;
        Ok(configuration)
    }

    async fn write_repository_configuration(
        &self,
        repository: &Path,
        configuration: &EffectiveConfiguration,
    ) -> ApplicationResult<PathBuf> {
        let document = render_document(configuration);
        let path = repository.join(REPOSITORY_CONFIGURATION_RELATIVE_PATH);
        write_atomic(&path, document.as_bytes())?;
        Ok(path)
    }

    async fn user_configuration_path(&self) -> ApplicationResult<PathBuf> {
        Ok(self.layout.user_configuration())
    }
}

pub fn render_document(configuration: &EffectiveConfiguration) -> String {
    let mut text = String::new();
    text.push_str(&format!("schema_version = {CONFIGURATION_SCHEMA_VERSION}\n\n"));
    text.push_str("[run]\n");
    text.push_str(&format!(
        "candidates = {}\n",
        configuration.budgets.candidates.get()
    ));
    text.push_str(&format!(
        "max_parallel_candidates = {}\n",
        configuration.budgets.max_parallel_candidates
    ));
    text.push_str(&format!(
        "max_repairs_per_candidate = {}\n",
        configuration.budgets.max_repairs_per_candidate
    ));
    text.push_str(&format!(
        "commit_policy = \"{}\"\n",
        configuration.commit_policy.as_str()
    ));
    text.push_str(&format!(
        "require_clean_repository = {}\n\n",
        configuration.git.require_clean_repository
    ));

    text.push_str("[agent]\n");
    text.push_str(&format!(
        "driver = \"{}\"\n",
        configuration.agent.driver.as_str()
    ));
    if let Some(model) = &configuration.agent.model {
        text.push_str(&format!("model = \"{model}\"\n"));
    }
    if let Some(endpoint) = &configuration.agent.endpoint {
        text.push_str(&format!("endpoint = \"{endpoint}\"\n"));
    }
    text.push_str(&format!("max_turns = {}\n", configuration.agent.max_turns));
    text.push_str(&format!(
        "timeout_seconds = {}\n",
        configuration.agent.timeout.get()
    ));
    text.push_str(&format!(
        "network = \"{}\"\n\n",
        configuration.agent.network.as_str()
    ));

    text.push_str("[quality]\n");
    text.push_str(&format!(
        "profile = \"{}\"\n",
        configuration.quality.profile.as_str()
    ));
    if let Some(coverage) = configuration.quality.minimum_line_coverage {
        text.push_str(&format!("minimum_line_coverage = {coverage}\n"));
    }
    text.push_str(&format!(
        "protect_existing_tests = {}\n\n",
        configuration.quality.protect_existing_tests
    ));

    for command in &configuration.commands.commands {
        text.push_str("[[commands]]\n");
        text.push_str(&format!("id = \"{}\"\n", command.id));
        text.push_str(&format!("program = \"{}\"\n", command.program));
        text.push_str(&format!(
            "args = [{}]\n",
            command
                .args
                .iter()
                .map(|value| format!("\"{value}\""))
                .collect::<Vec<_>>()
                .join(", ")
        ));
        text.push_str(&format!("kind = \"{}\"\n", command.kind.as_str()));
        text.push_str(&format!("timeout_seconds = {}\n", command.timeout.get()));
        text.push_str(&format!("required = {}\n", command.required));
        if command.report_format != ReportFormat::None {
            text.push_str(&format!(
                "report_format = \"{}\"\n",
                command.report_format.as_str()
            ));
        }
        if let Some(path) = &command.report_path {
            text.push_str(&format!("report_path = \"{path}\"\n"));
        }
        text.push('\n');
    }

    text.push_str("[git]\n");
    text.push_str(&format!(
        "branch_prefix = \"{}\"\n",
        configuration.git.branch_prefix
    ));
    text.push_str(&format!(
        "author_name = \"{}\"\n\n",
        configuration.git.author_name
    ));

    text.push_str("[policy]\n");
    text.push_str(&format!(
        "protected_paths = [{}]\n",
        configuration
            .path_policy
            .protected_patterns
            .iter()
            .map(|value| format!("\"{value}\""))
            .collect::<Vec<_>>()
            .join(", ")
    ));
    text
}
