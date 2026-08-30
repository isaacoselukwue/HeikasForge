use std::path::Path;

use crate::configuration::{RepositoryTrustState, WithheldReason};
use crate::error::ApplicationResult;
use crate::model::doctor::{AdapterStatus, DoctorCheck, DoctorReport};
use crate::usecases::service::ApplicationService;

const MINIMUM_FREE_BYTES: u64 = 2_147_483_648;

pub async fn diagnose(
    service: &ApplicationService,
    repository: Option<&Path>,
) -> ApplicationResult<DoctorReport> {
    let base = service.base();
    let mut checks = Vec::new();
    let mut adapters = Vec::new();

    let host = base.host.facts().await?;
    checks.push(DoctorCheck::passed(
        "host",
        "Environment",
        "Host platform",
        format!(
            "{} on {} with {} logical processors",
            host.operating_system, host.architecture, host.logical_processors
        ),
    ));

    if host.data_root_writable {
        checks.push(DoctorCheck::passed(
            "data-root",
            "Environment",
            "Application data root",
            format!("{} is writable", host.heikas_home.display()),
        ));
    } else {
        checks.push(DoctorCheck::failed(
            "data-root",
            "Environment",
            "Application data root",
            format!("{} is not writable", host.heikas_home.display()),
            "Set HEIKAS_HOME to a writable directory or correct the permissions.",
        ));
    }

    match base.host.disk_space(&host.heikas_home).await {
        Ok(space) if space.available_bytes >= MINIMUM_FREE_BYTES => {
            checks.push(DoctorCheck::passed(
                "disk-space",
                "Environment",
                "Disk space",
                format!("{} bytes available", space.available_bytes),
            ));
        }
        Ok(space) => checks.push(DoctorCheck::warning(
            "disk-space",
            "Environment",
            "Disk space",
            format!(
                "{} bytes available, which is below the recommended {} bytes",
                space.available_bytes, MINIMUM_FREE_BYTES
            ),
            "Free disk space before starting a run with several candidates.",
        )),
        Err(error) => checks.push(DoctorCheck::warning(
            "disk-space",
            "Environment",
            "Disk space",
            error.to_string(),
            "Confirm the application data root exists and is readable.",
        )),
    }

    match base.processes.probe_executable("git").await? {
        Some(version) => checks.push(DoctorCheck::passed(
            "git-executable",
            "Git",
            "Git executable",
            version,
        )),
        None => checks.push(DoctorCheck::failed(
            "git-executable",
            "Git",
            "Git executable",
            "the `git` executable was not found on the path",
            "Install Git and ensure it is on the executable search path.",
        )),
    }

    let configuration = match repository {
        Some(path) => match base.git.inspect(path).await {
            Ok(facts) => {
                checks.push(DoctorCheck::passed(
                    "repository",
                    "Git",
                    "Repository",
                    format!(
                        "{} at {} on {}",
                        facts.root.display(),
                        facts.head_commit.short(),
                        facts.default_branch
                    ),
                ));
                if facts.is_clean {
                    checks.push(DoctorCheck::passed(
                        "repository-clean",
                        "Git",
                        "Working tree",
                        "the working tree is clean",
                    ));
                } else {
                    checks.push(DoctorCheck::warning(
                        "repository-clean",
                        "Git",
                        "Working tree",
                        format!(
                            "{} staged, {} unstaged and {} untracked paths",
                            facts.staged_paths.len(),
                            facts.unstaged_paths.len(),
                            facts.untracked_paths.len()
                        ),
                        "Commit or stash the changes, or start the run with the include dirty option.",
                    ));
                }
                match facts.configured_user_email.as_deref() {
                    Some(email) if !email.trim().is_empty() => {
                        checks.push(DoctorCheck::passed(
                            "git-identity",
                            "Git",
                            "Commit identity",
                            format!(
                                "commits will use `Isaac Oselukwue` with the repository email `{email}`"
                            ),
                        ));
                    }
                    _ => checks.push(DoctorCheck::failed(
                        "git-identity",
                        "Git",
                        "Commit identity",
                        "the repository has no configured Git email",
                        "Set `git config user.email` in the repository before requesting a commit.",
                    )),
                }
                if facts.signing_enabled {
                    checks.push(DoctorCheck::warning(
                        "git-signing",
                        "Git",
                        "Commit signing",
                        "commit signing is enabled in this repository",
                        "Confirm the signing key works without an interactive prompt.",
                    ));
                }
                match service.configuration_resolver().detect(path).await {
                    Ok(configuration) => Some(configuration),
                    Err(error) => {
                        checks.push(DoctorCheck::failed(
                            "configuration",
                            "Configuration",
                            "Effective configuration",
                            error.to_string(),
                            "Correct `.heikas/forge.toml` or run `heikas init`.",
                        ));
                        None
                    }
                }
            }
            Err(error) => {
                checks.push(DoctorCheck::failed(
                    "repository",
                    "Git",
                    "Repository",
                    error.to_string(),
                    "Select a directory inside a Git working tree.",
                ));
                None
            }
        },
        None => None,
    };

    if let Some(configuration) = &configuration {
        let trust = &configuration.repository_trust;
        match trust.state {
            RepositoryTrustState::NoRepositoryConfiguration => {
                checks.push(DoctorCheck::passed(
                    "repository-trust",
                    "Configuration",
                    "Repository configuration trust",
                    "the repository declares no configuration, so only your own settings apply",
                ));
            }
            RepositoryTrustState::Trusted => {
                checks.push(DoctorCheck::passed(
                    "repository-trust",
                    "Configuration",
                    "Repository configuration trust",
                    format!(
                        "the repository configuration is trusted at digest {}",
                        trust
                            .configuration_digest
                            .as_ref()
                            .map(|digest| digest.short().to_string())
                            .unwrap_or_default()
                    ),
                ));
            }
            RepositoryTrustState::Untrusted => {
                checks.push(DoctorCheck::warning(
                    "repository-trust",
                    "Configuration",
                    "Repository configuration trust",
                    "the repository configuration has not been trusted, so the settings that name executables were withheld",
                    "Read `.heikas/forge.toml`, then run `heikas trust <repository>` if you accept the commands it declares.",
                ));
            }
        }
        for withheld in &trust.withheld {
            checks.push(DoctorCheck::warning(
                &format!("repository-trust-{}", withheld.setting.replace('.', "-")),
                "Configuration",
                &format!("Withheld setting `{}`", withheld.setting),
                withheld.reason.explanation(),
                match withheld.reason {
                    WithheldReason::RequiresRepositoryTrust => {
                        "Run `heikas trust <repository>` after reviewing `.heikas/forge.toml`."
                    }
                    WithheldReason::UserConfigurationOnly => {
                        "Move the setting into your own user configuration if you intend it."
                    }
                    WithheldReason::WouldWeakenPolicy => {
                        "Relax the setting in your own user configuration if you intend it."
                    }
                },
            ));
        }

        match configuration.validate() {
            Ok(()) => checks.push(DoctorCheck::passed(
                "configuration",
                "Configuration",
                "Effective configuration",
                format!(
                    "{} commands, {} quality profile, {} candidates",
                    configuration.commands.commands.len(),
                    configuration.quality.profile.as_str(),
                    configuration.budgets.candidates.get()
                ),
            )),
            Err(error) => checks.push(DoctorCheck::failed(
                "configuration",
                "Configuration",
                "Effective configuration",
                error.to_string(),
                "The detail above names every command kind that is missing and the exact flags that declare them.",
            )),
        }

        for command in &configuration.commands.commands {
            match base.processes.probe_executable(&command.program).await? {
                Some(version) => checks.push(DoctorCheck::passed(
                    &format!("command-{}", command.id),
                    "Commands",
                    &format!("Command `{}`", command.id),
                    format!("{} resolves to {}", command.display_line(), version),
                )),
                None => checks.push(DoctorCheck::failed(
                    &format!("command-{}", command.id),
                    "Commands",
                    &format!("Command `{}`", command.id),
                    format!("the executable `{}` was not found", command.program),
                    "Install the executable or edit the command in `.heikas/forge.toml`.",
                )),
            }
        }

        let factory = service.runtime_factory();
        let capabilities = match factory.agent_driver(configuration).await {
            Ok(driver) => match driver.capabilities().await {
                Ok(capabilities) => Some(capabilities),
                Err(error) => {
                    checks.push(DoctorCheck::failed(
                        "agent",
                        "Agent",
                        "Agent driver",
                        error.to_string(),
                        "Start the local model runtime or select a different driver.",
                    ));
                    None
                }
            },
            Err(error) => {
                checks.push(DoctorCheck::failed(
                    "agent",
                    "Agent",
                    "Agent driver",
                    error.to_string(),
                    "Correct the agent configuration in `.heikas/forge.toml`.",
                ));
                adapters.push(AdapterStatus {
                    name: configuration.agent.driver.label().to_string(),
                    kind: "agent".to_string(),
                    available: false,
                    version: None,
                    requires_paid_account: configuration.agent.driver.requires_paid_account(),
                    isolation: None,
                    detail: error.to_string(),
                });
                None
            }
        };

        if let Some(capabilities) = capabilities {
            adapters.push(AdapterStatus {
                name: capabilities.driver.label().to_string(),
                kind: "agent".to_string(),
                available: capabilities.available,
                version: capabilities.version.clone(),
                requires_paid_account: capabilities.requires_paid_account,
                isolation: Some(capabilities.isolation.label().to_string()),
                detail: capabilities.diagnostics.join("; "),
            });
            if capabilities.available && capabilities.supports_structured_tool_calls {
                checks.push(DoctorCheck::passed(
                    "agent",
                    "Agent",
                    "Agent driver",
                    format!(
                        "{} is available with {} isolation",
                        capabilities.driver.label(),
                        capabilities.isolation.label()
                    ),
                ));
            } else if capabilities.available {
                checks.push(DoctorCheck::failed(
                    "agent",
                    "Agent",
                    "Agent driver",
                    "the driver is available but does not support reliable structured tool calls",
                    "Select a model that supports structured tool calling.",
                ));
            } else {
                checks.push(DoctorCheck::failed(
                    "agent",
                    "Agent",
                    "Agent driver",
                    capabilities.diagnostics.join("; "),
                    "Start the local model runtime or select a different driver.",
                ));
            }
        }

        let providers = match factory.review_providers(configuration).await {
            Ok(providers) => providers,
            Err(error) => {
                checks.push(DoctorCheck::failed(
                    "review-providers",
                    "Quality",
                    "Review providers",
                    error.to_string(),
                    "Correct the quality configuration in `.heikas/forge.toml`.",
                ));
                Vec::new()
            }
        };
        for provider in providers {
            let available = provider.available().await.unwrap_or(false);
            adapters.push(AdapterStatus {
                name: provider.name().to_string(),
                kind: "review".to_string(),
                available,
                version: None,
                requires_paid_account: false,
                isolation: None,
                detail: if provider.required() {
                    "required quality provider".to_string()
                } else {
                    "advisory quality provider".to_string()
                },
            });
            if provider.required() && !available {
                checks.push(DoctorCheck::failed(
                    &format!("review-{}", provider.name()),
                    "Quality",
                    &format!("Review provider `{}`", provider.name()),
                    "the required review provider is not available",
                    "Install the provider or change the quality profile.",
                ));
            } else {
                checks.push(DoctorCheck::passed(
                    &format!("review-{}", provider.name()),
                    "Quality",
                    &format!("Review provider `{}`", provider.name()),
                    if available {
                        "available".to_string()
                    } else {
                        "not available and advisory only".to_string()
                    },
                ));
            }
        }
    } else {
        checks.push(DoctorCheck::skipped(
            "configuration",
            "Configuration",
            "Effective configuration",
            "no repository was supplied, so repository configuration was not inspected",
        ));
    }

    let free_path_available = adapters.iter().any(|adapter| {
        adapter.kind == "agent" && adapter.available && !adapter.requires_paid_account
    });

    let mut report = DoctorReport {
        repository_path: repository.map(|path| path.display().to_string()),
        checks,
        adapters,
        ready: true,
        free_path_available,
    };
    report.recompute();
    Ok(report)
}
