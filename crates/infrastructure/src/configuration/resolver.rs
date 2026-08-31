use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Arc;

use async_trait::async_trait;
use heikas_application::configuration::{
    AgentConfiguration, AgentDriverKind, AiReviewConfiguration, CommandCatalogueSource,
    ConfigurationAuthority, EffectiveConfiguration, GitConfiguration, QualityConfiguration,
    RedactionConfiguration, RepositoryTrustDecision, RepositoryTrustRecord, RepositoryTrustState,
    SonarMcpConfiguration, SonarScannerConfiguration, WithheldReason, WithheldSetting,
    CONFIGURATION_SCHEMA_VERSION, REPOSITORY_CONFIGURATION_RELATIVE_PATH,
};
use heikas_application::error::{ApplicationError, ApplicationResult};
use heikas_application::model::request::CreateRunRequest;
use heikas_application::ports::clock::Clock;
use heikas_application::ports::git::GitService;
use heikas_application::ports::runtime::ConfigurationResolver;
use heikas_domain::budget::{CandidateCount, QualityProfile, RunBudgets};
use heikas_domain::clock::TimeoutSeconds;
use heikas_domain::command::{
    CommandCatalogue, CommandId, CommandKind, CommandSpecification, ReportFormat,
    MAXIMUM_COMMAND_TIMEOUT_SECONDS,
};
use heikas_domain::identity::ContentDigest;
use heikas_domain::path_policy::PathPolicy;
use heikas_domain::retry::{NodeTimeouts, RetryPolicy};
use heikas_domain::run::CommitPolicy;

use crate::atomic::write_atomic;
use crate::configuration::detection::{survey_project, ProjectSurvey, SURVEYED_MARKERS};
use crate::configuration::document::{CommandSection, ForgeDocument};
use crate::configuration::trust::FileRepositoryTrustStore;
use crate::layout::StoreLayout;
use crate::process::supervisor::essential_environment_variables;

struct AuthorityGate {
    authority: ConfigurationAuthority,
    trusted: bool,
    withheld: Vec<WithheldSetting>,
}

impl AuthorityGate {
    fn user() -> Self {
        Self {
            authority: ConfigurationAuthority::UserConfiguration,
            trusted: true,
            withheld: Vec::new(),
        }
    }

    fn repository(trusted: bool) -> Self {
        Self {
            authority: ConfigurationAuthority::Repository,
            trusted,
            withheld: Vec::new(),
        }
    }

    fn is_user(&self) -> bool {
        self.authority == ConfigurationAuthority::UserConfiguration
    }

    fn withhold(&mut self, setting: &str, reason: WithheldReason) {
        if self
            .withheld
            .iter()
            .any(|entry| entry.setting == setting && entry.reason == reason)
        {
            return;
        }
        self.withheld.push(WithheldSetting {
            setting: setting.to_string(),
            reason,
        });
    }

    fn owner_only(&mut self, setting: &str) -> bool {
        if self.is_user() {
            return true;
        }
        self.withhold(setting, WithheldReason::UserConfigurationOnly);
        false
    }

    fn requires_trust(&mut self, setting: &str) -> bool {
        if self.is_user() || self.trusted {
            return true;
        }
        self.withhold(setting, WithheldReason::RequiresRepositoryTrust);
        false
    }

    fn never_relaxes(&mut self, setting: &str, would_relax: bool) -> bool {
        if self.is_user() || !would_relax {
            return true;
        }
        self.withhold(setting, WithheldReason::WouldWeakenPolicy);
        false
    }
}

const MAXIMUM_SURVEYED_PATHS: usize = 20_000;

pub struct LayeredConfigurationResolver {
    layout: StoreLayout,
    trust: FileRepositoryTrustStore,
    clock: Arc<dyn Clock>,
    git: Arc<dyn GitService>,
}

impl LayeredConfigurationResolver {
    pub fn new(layout: StoreLayout, clock: Arc<dyn Clock>, git: Arc<dyn GitService>) -> Self {
        let trust = FileRepositoryTrustStore::new(&layout);
        Self {
            layout,
            trust,
            clock,
            git,
        }
    }

    async fn survey(&self, repository: &Path) -> ProjectSurvey {
        let tracked = self
            .git
            .list_paths(repository, MAXIMUM_SURVEYED_PATHS)
            .await
            .ok();
        survey_project(repository, tracked.as_deref())
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
            repository_trust: RepositoryTrustDecision::default(),
            command_source: CommandCatalogueSource::default(),
            detection_notes: Vec::new(),
        }
    }

    fn read_bytes(path: &Path) -> ApplicationResult<Option<Vec<u8>>> {
        match std::fs::read(path) {
            Ok(contents) => Ok(Some(contents)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(crate::atomic::storage(path, "read", error)),
        }
    }

    fn read_repository_bytes(repository: &Path) -> ApplicationResult<Option<Vec<u8>>> {
        crate::paths::read_confined_file(
            repository,
            REPOSITORY_CONFIGURATION_RELATIVE_PATH,
            crate::paths::MAXIMUM_REPOSITORY_CONFIGURATION_BYTES,
        )
    }

    fn parse_document(path: &Path, bytes: &[u8]) -> ApplicationResult<ForgeDocument> {
        let contents = std::str::from_utf8(bytes).map_err(|error| {
            ApplicationError::InvalidConfiguration(format!(
                "`{}` is not valid UTF-8: {error}",
                path.display()
            ))
        })?;
        let document: ForgeDocument = toml::from_str(contents).map_err(|error| {
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
        Ok(document)
    }

    fn read_document(path: &Path) -> ApplicationResult<Option<ForgeDocument>> {
        match Self::read_bytes(path)? {
            Some(bytes) => Ok(Some(Self::parse_document(path, &bytes)?)),
            None => Ok(None),
        }
    }

    fn apply(
        configuration: &mut EffectiveConfiguration,
        document: &ForgeDocument,
        gate: &mut AuthorityGate,
    ) -> ApplicationResult<()> {
        if let Some(run) = &document.run {
            if let Some(candidates) = run.candidates {
                let parsed = CandidateCount::new(candidates)?;
                let relaxes = parsed.get() > configuration.budgets.candidates.get();
                if gate.never_relaxes("run.candidates", relaxes) {
                    configuration.budgets.candidates = parsed;
                }
            }
            if let Some(parallel) = run.max_parallel_candidates {
                let relaxes = parallel > configuration.budgets.max_parallel_candidates;
                if gate.never_relaxes("run.max_parallel_candidates", relaxes) {
                    configuration.budgets.max_parallel_candidates = parallel;
                }
            }
            if let Some(repairs) = run.max_repairs_per_candidate {
                let relaxes = repairs > configuration.budgets.max_repairs_per_candidate;
                if gate.never_relaxes("run.max_repairs_per_candidate", relaxes) {
                    configuration.budgets.max_repairs_per_candidate = repairs;
                }
            }
            if let Some(seconds) = run.wall_clock_seconds {
                let relaxes = seconds > configuration.budgets.wall_clock_seconds;
                if gate.never_relaxes("run.wall_clock_seconds", relaxes) {
                    configuration.budgets.wall_clock_seconds = seconds;
                }
            }
            if let Some(bytes) = run.max_output_bytes_per_stream {
                let relaxes = bytes > configuration.budgets.max_output_bytes_per_stream;
                if gate.never_relaxes("run.max_output_bytes_per_stream", relaxes) {
                    configuration.budgets.max_output_bytes_per_stream = bytes;
                }
            }
            if let Some(policy) = &run.commit_policy {
                if gate.owner_only("run.commit_policy") {
                    configuration.commit_policy = CommitPolicy::from_str(policy)?;
                }
            }
            if let Some(require_clean) = run.require_clean_repository {
                let relaxes = configuration.git.require_clean_repository && !require_clean;
                if gate.never_relaxes("run.require_clean_repository", relaxes) {
                    configuration.git.require_clean_repository = require_clean;
                }
            }
        }
        if let Some(agent) = &document.agent {
            if let Some(driver) = &agent.driver {
                if gate.requires_trust("agent.driver") {
                    configuration.agent.driver = AgentDriverKind::from_str(driver)?;
                }
            }
            if agent.model.is_some() && gate.owner_only("agent.model") {
                configuration.agent.model = agent.model.clone();
            }
            if agent.endpoint.is_some() && gate.owner_only("agent.endpoint") {
                configuration.agent.endpoint = agent.endpoint.clone();
            }
            if agent.api_key_environment_variable.is_some()
                && gate.owner_only("agent.api_key_environment_variable")
            {
                configuration.agent.api_key_environment_variable =
                    agent.api_key_environment_variable.clone();
            }
            if agent.executable.is_some() && gate.owner_only("agent.executable") {
                configuration.agent.executable = agent.executable.clone();
            }
            if let Some(extra) = &agent.extra_arguments {
                if gate.owner_only("agent.extra_arguments") {
                    configuration.agent.extra_arguments = extra.clone();
                }
            }
            if let Some(turns) = agent.max_turns {
                let relaxes = turns > configuration.agent.max_turns;
                if gate.never_relaxes("agent.max_turns", relaxes) {
                    configuration.agent.max_turns = turns;
                    configuration.budgets.max_agent_turns = turns;
                }
            }
            if let Some(seconds) = agent.timeout_seconds {
                let relaxes = seconds > configuration.timeouts.agent_seconds;
                if gate.never_relaxes("agent.timeout_seconds", relaxes) {
                    configuration.agent.timeout =
                        TimeoutSeconds::clamped(seconds, MAXIMUM_COMMAND_TIMEOUT_SECONDS);
                    configuration.timeouts.agent_seconds = seconds;
                }
            }
            if let Some(network) = agent.network {
                let relaxes = network.permissiveness_rank()
                    > configuration.agent.network.permissiveness_rank();
                if gate.never_relaxes("agent.network", relaxes) {
                    configuration.agent.network = network;
                }
            }
            if agent.fixture_script.is_some() && gate.requires_trust("agent.fixture_script") {
                configuration.agent.fixture_script = agent.fixture_script.clone();
            }
        }
        if let Some(quality) = &document.quality {
            if let Some(profile) = &quality.profile {
                let parsed = QualityProfile::from_str(profile)?;
                let relaxes =
                    parsed.strictness_rank() < configuration.quality.profile.strictness_rank();
                if gate.never_relaxes("quality.profile", relaxes) {
                    configuration.quality.profile = parsed;
                }
            }
            if let Some(coverage) = quality.minimum_line_coverage {
                let relaxes = configuration
                    .quality
                    .minimum_line_coverage
                    .is_some_and(|current| coverage < current);
                if gate.never_relaxes("quality.minimum_line_coverage", relaxes) {
                    configuration.quality.minimum_line_coverage = Some(coverage);
                }
            }
            if let Some(protect) = quality.protect_existing_tests {
                let relaxes = configuration.quality.protect_existing_tests && !protect;
                if gate.never_relaxes("quality.protect_existing_tests", relaxes) {
                    configuration.quality.protect_existing_tests = protect;
                }
            }
            if let Some(scanner) = &quality.sonar_scanner {
                apply_scanner(&mut configuration.quality.sonar_scanner, scanner, gate);
            }
            if let Some(mcp) = &quality.sonar_mcp {
                apply_mcp(&mut configuration.quality.sonar_mcp, mcp, gate);
            }
            if let Some(ai) = &quality.ai_review {
                apply_ai(&mut configuration.quality.ai_review, ai, gate);
            }
        }
        if let Some(git) = &document.git {
            if let Some(prefix) = &git.branch_prefix {
                if gate.owner_only("git.branch_prefix") {
                    configuration.git.branch_prefix = prefix.clone();
                }
            }
            if let Some(author) = &git.author_name {
                if gate.owner_only("git.author_name") {
                    configuration.git.author_name = author.clone();
                }
            }
            if let Some(include_dirty) = git.include_dirty {
                if gate.owner_only("git.include_dirty") {
                    configuration.git.include_dirty = include_dirty;
                }
            }
        }
        if let Some(policy) = &document.policy {
            if let Some(protected) = &policy.protected_paths {
                configuration.path_policy.protected_patterns = if gate.is_user() {
                    protected.clone()
                } else {
                    union(&configuration.path_policy.protected_patterns, protected)
                };
            }
            if let Some(sensitive) = &policy.sensitive_paths {
                configuration.path_policy.sensitive_patterns = if gate.is_user() {
                    sensitive.clone()
                } else {
                    union(&configuration.path_policy.sensitive_patterns, sensitive)
                };
            }
            if let Some(bytes) = policy.maximum_read_bytes {
                configuration.path_policy.maximum_read_bytes = if gate.is_user() {
                    bytes
                } else {
                    bytes.min(configuration.path_policy.maximum_read_bytes)
                };
            }
            if let Some(bytes) = policy.maximum_write_bytes {
                configuration.path_policy.maximum_write_bytes = if gate.is_user() {
                    bytes
                } else {
                    bytes.min(configuration.path_policy.maximum_write_bytes)
                };
            }
        }
        if let Some(redaction) = &document.redaction {
            if let Some(variables) = &redaction.secret_environment_variables {
                configuration.redaction.secret_environment_variables = if gate.is_user() {
                    variables.clone()
                } else {
                    union(
                        &configuration.redaction.secret_environment_variables,
                        variables,
                    )
                };
            }
            if let Some(patterns) = &redaction.additional_patterns {
                configuration.redaction.additional_patterns = if gate.is_user() {
                    patterns.clone()
                } else {
                    union(&configuration.redaction.additional_patterns, patterns)
                };
            }
            if let Some(redact_home) = redaction.redact_home_prefix {
                let relaxes = configuration.redaction.redact_home_prefix && !redact_home;
                if gate.never_relaxes("redaction.redact_home_prefix", relaxes) {
                    configuration.redaction.redact_home_prefix = redact_home;
                }
            }
        }
        if let Some(environment) = &document.environment {
            if let Some(allowlist) = &environment.allowlist {
                if gate.is_user() {
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
                } else {
                    let unknown: Vec<&String> = allowlist
                        .iter()
                        .filter(|name| !configuration.environment_allowlist.contains(name))
                        .collect();
                    if !unknown.is_empty() {
                        gate.withhold("environment.allowlist", WithheldReason::WouldWeakenPolicy);
                    }
                }
            }
        }
        if let Some(commands) = &document.commands {
            if gate.requires_trust("commands") {
                let mut catalogue = CommandCatalogue::default();
                for section in commands {
                    catalogue.commands.push(convert_command(section)?);
                }
                configuration.commands = catalogue;
                configuration.command_source = if gate.is_user() {
                    CommandCatalogueSource::UserConfiguration
                } else {
                    CommandCatalogueSource::RepositoryConfiguration
                };
            }
        }
        Ok(())
    }

    async fn resolve_trust(
        &self,
        repository: &Path,
        digest: &ContentDigest,
    ) -> ApplicationResult<bool> {
        Ok(self
            .trust
            .record_for(repository)?
            .is_some_and(|record| &record.configuration_digest == digest))
    }
}

fn union(current: &[String], additional: &[String]) -> Vec<String> {
    let mut merged = current.to_vec();
    for value in additional {
        if !merged.contains(value) {
            merged.push(value.clone());
        }
    }
    merged
}

fn apply_scanner(
    target: &mut SonarScannerConfiguration,
    section: &crate::configuration::document::SonarScannerSection,
    gate: &mut AuthorityGate,
) {
    if let Some(enabled) = section.enabled {
        let relaxes = target.enabled && !enabled;
        if gate.never_relaxes("quality.sonar_scanner.enabled", relaxes) {
            target.enabled = enabled;
        }
    }
    if let Some(program) = &section.program {
        if gate.requires_trust("quality.sonar_scanner.program") {
            target.program = program.clone();
        }
    }
    if let Some(arguments) = &section.arguments {
        if gate.requires_trust("quality.sonar_scanner.arguments") {
            target.arguments = arguments.clone();
        }
    }
    if let Some(host) = &section.host_url {
        if gate.owner_only("quality.sonar_scanner.host_url") {
            target.host_url = host.clone();
        }
    }
    if section.project_key.is_some() {
        target.project_key = section.project_key.clone();
    }
    if section.token_environment_variable.is_some()
        && gate.owner_only("quality.sonar_scanner.token_environment_variable")
    {
        target.token_environment_variable = section.token_environment_variable.clone();
    }
    if let Some(wait) = section.wait_for_quality_gate {
        let relaxes = target.wait_for_quality_gate && !wait;
        if gate.never_relaxes("quality.sonar_scanner.wait_for_quality_gate", relaxes) {
            target.wait_for_quality_gate = wait;
        }
    }
    if let Some(seconds) = section.timeout_seconds {
        target.timeout = TimeoutSeconds::clamped(seconds, MAXIMUM_COMMAND_TIMEOUT_SECONDS);
    }
}

fn apply_mcp(
    target: &mut SonarMcpConfiguration,
    section: &crate::configuration::document::SonarMcpSection,
    gate: &mut AuthorityGate,
) {
    if let Some(enabled) = section.enabled {
        let relaxes = target.enabled && !enabled;
        if gate.never_relaxes("quality.sonar_mcp.enabled", relaxes) {
            target.enabled = enabled;
        }
    }
    if let Some(program) = &section.program {
        if gate.requires_trust("quality.sonar_mcp.program") {
            target.program = program.clone();
        }
    }
    if let Some(arguments) = &section.arguments {
        if gate.requires_trust("quality.sonar_mcp.arguments") {
            target.arguments = arguments.clone();
        }
    }
    if section.token_environment_variable.is_some()
        && gate.owner_only("quality.sonar_mcp.token_environment_variable")
    {
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
    gate: &mut AuthorityGate,
) {
    if let Some(enabled) = section.enabled {
        target.enabled = enabled;
    }
    if let Some(advisory) = section.advisory_only {
        let relaxes = !target.advisory_only && advisory;
        if gate.never_relaxes("quality.ai_review.advisory_only", relaxes) {
            target.advisory_only = advisory;
        }
    }
    if let Some(rules) = &section.gate_rules {
        target.gate_rules = if gate.is_user() {
            rules.clone()
        } else {
            union(&target.gate_rules, rules)
        };
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
        success_exit_codes: section
            .success_exit_codes
            .clone()
            .unwrap_or_else(|| vec![0]),
    };
    specification.validate()?;
    Ok(specification)
}

#[async_trait]
impl ConfigurationResolver for LayeredConfigurationResolver {
    async fn detect(&self, repository: &Path) -> ApplicationResult<EffectiveConfiguration> {
        let mut configuration = Self::base_configuration(repository);
        let user_path = self.layout.user_configuration();
        if let Some(document) = Self::read_document(&user_path)? {
            Self::apply(&mut configuration, &document, &mut AuthorityGate::user())?;
        }
        let repository_path = repository.join(REPOSITORY_CONFIGURATION_RELATIVE_PATH);
        match Self::read_repository_bytes(repository)? {
            Some(bytes) => {
                let digest = ContentDigest::of_bytes(&bytes);
                let document = Self::parse_document(&repository_path, &bytes)?;
                let trusted = self.resolve_trust(repository, &digest).await?;
                let mut gate = AuthorityGate::repository(trusted);
                Self::apply(&mut configuration, &document, &mut gate)?;
                configuration.repository_trust = RepositoryTrustDecision {
                    state: if trusted {
                        RepositoryTrustState::Trusted
                    } else {
                        RepositoryTrustState::Untrusted
                    },
                    configuration_digest: Some(digest),
                    withheld: gate.withheld,
                };
            }
            None => {
                configuration.repository_trust = RepositoryTrustDecision::default();
            }
        }
        if configuration.commands.commands.is_empty() {
            let survey = self.survey(repository).await;
            configuration.command_source = if !survey.tracked_listing_available {
                CommandCatalogueSource::NotSurveyed(
                    "the tracked file listing could not be read".to_string(),
                )
            } else if survey.modules.is_empty() {
                CommandCatalogueSource::NothingDetected(
                    SURVEYED_MARKERS
                        .iter()
                        .map(|marker| marker.to_string())
                        .collect(),
                )
            } else {
                CommandCatalogueSource::Detected(survey.describe_kinds())
            };
            configuration.detection_notes = if survey.tracked_listing_available {
                survey
                    .declines
                    .iter()
                    .map(|decline| format!("{}: {}", decline.subject, decline.detail))
                    .collect()
            } else {
                Vec::new()
            };
            configuration.commands = CommandCatalogue {
                commands: survey.commands,
            };
        }
        Ok(configuration)
    }

    async fn resolve(
        &self,
        request: &CreateRunRequest,
    ) -> ApplicationResult<EffectiveConfiguration> {
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
        self.trust.grant(
            repository,
            ContentDigest::of_str(&document),
            self.clock.now(),
        )?;
        Ok(path)
    }

    async fn user_configuration_path(&self) -> ApplicationResult<PathBuf> {
        Ok(self.layout.user_configuration())
    }

    async fn repository_trust(
        &self,
        repository: &Path,
    ) -> ApplicationResult<RepositoryTrustDecision> {
        Ok(self.detect(repository).await?.repository_trust)
    }

    async fn trust_repository(
        &self,
        repository: &Path,
    ) -> ApplicationResult<RepositoryTrustRecord> {
        let path = repository.join(REPOSITORY_CONFIGURATION_RELATIVE_PATH);
        let Some(bytes) = Self::read_repository_bytes(repository)? else {
            return Err(ApplicationError::InvalidConfiguration(format!(
                "`{}` does not exist, so there is nothing to trust",
                path.display()
            )));
        };
        Self::parse_document(&path, &bytes)?;
        self.trust.grant(
            repository,
            ContentDigest::of_bytes(&bytes),
            self.clock.now(),
        )
    }

    async fn revoke_repository_trust(&self, repository: &Path) -> ApplicationResult<bool> {
        self.trust.revoke(repository)
    }

    async fn trusted_repositories(&self) -> ApplicationResult<Vec<RepositoryTrustRecord>> {
        self.trust.records()
    }
}

fn toml_string(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len() + 2);
    encoded.push('"');
    for character in value.chars() {
        match character {
            '"' => encoded.push_str("\\\""),
            '\\' => encoded.push_str("\\\\"),
            '\n' => encoded.push_str("\\n"),
            '\r' => encoded.push_str("\\r"),
            '\t' => encoded.push_str("\\t"),
            control if control.is_control() => {
                encoded.push_str(&format!("\\u{:04X}", control as u32));
            }
            other => encoded.push(other),
        }
    }
    encoded.push('"');
    encoded
}

fn toml_string_array(values: impl IntoIterator<Item = String>) -> String {
    let encoded: Vec<String> = values
        .into_iter()
        .map(|value| toml_string(&value))
        .collect();
    format!("[{}]", encoded.join(", "))
}

pub fn render_document(configuration: &EffectiveConfiguration) -> String {
    let mut text = String::new();
    text.push_str(&format!(
        "schema_version = {CONFIGURATION_SCHEMA_VERSION}\n\n"
    ));
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
        "commit_policy = {}\n",
        toml_string(configuration.commit_policy.as_str())
    ));
    text.push_str(&format!(
        "require_clean_repository = {}\n\n",
        configuration.git.require_clean_repository
    ));

    text.push_str("[agent]\n");
    text.push_str(&format!(
        "driver = {}\n",
        toml_string(configuration.agent.driver.as_str())
    ));
    if let Some(model) = &configuration.agent.model {
        text.push_str(&format!("model = {}\n", toml_string(model)));
    }
    if let Some(endpoint) = &configuration.agent.endpoint {
        text.push_str(&format!("endpoint = {}\n", toml_string(endpoint)));
    }
    text.push_str(&format!("max_turns = {}\n", configuration.agent.max_turns));
    text.push_str(&format!(
        "timeout_seconds = {}\n",
        configuration.agent.timeout.get()
    ));
    text.push_str(&format!(
        "network = {}\n\n",
        toml_string(configuration.agent.network.as_str())
    ));

    text.push_str("[quality]\n");
    text.push_str(&format!(
        "profile = {}\n",
        toml_string(configuration.quality.profile.as_str())
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
        text.push_str(&format!("id = {}\n", toml_string(command.id.as_str())));
        text.push_str(&format!("program = {}\n", toml_string(&command.program)));
        text.push_str(&format!(
            "args = {}\n",
            toml_string_array(command.args.iter().cloned())
        ));
        text.push_str(&format!("kind = {}\n", toml_string(command.kind.as_str())));
        text.push_str(&format!("timeout_seconds = {}\n", command.timeout.get()));
        text.push_str(&format!("required = {}\n", command.required));
        if let Some(subdirectory) = &command.working_subdirectory {
            text.push_str(&format!(
                "working_subdirectory = {}\n",
                toml_string(subdirectory)
            ));
        }
        if command.success_exit_codes != vec![0] {
            text.push_str(&format!(
                "success_exit_codes = [{}]\n",
                command
                    .success_exit_codes
                    .iter()
                    .map(|code| code.to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        if !command.environment.is_empty() {
            text.push_str(&format!(
                "environment = [{}]\n",
                command
                    .environment
                    .iter()
                    .map(|(name, value)| format!("[{}, {}]", toml_string(name), toml_string(value)))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        if command.report_format != ReportFormat::None {
            text.push_str(&format!(
                "report_format = {}\n",
                toml_string(command.report_format.as_str())
            ));
        }
        if let Some(path) = &command.report_path {
            text.push_str(&format!("report_path = {}\n", toml_string(path)));
        }
        text.push('\n');
    }

    text.push_str("[git]\n");
    text.push_str(&format!(
        "branch_prefix = {}\n",
        toml_string(&configuration.git.branch_prefix)
    ));
    text.push_str(&format!(
        "author_name = {}\n\n",
        toml_string(&configuration.git.author_name)
    ));

    text.push_str("[policy]\n");
    text.push_str(&format!(
        "protected_paths = {}\n",
        toml_string_array(configuration.path_policy.protected_patterns.iter().cloned())
    ));
    text.push_str(&format!(
        "sensitive_paths = {}\n",
        toml_string_array(configuration.path_policy.sensitive_patterns.iter().cloned())
    ));
    text
}
