use std::sync::Arc;

use async_trait::async_trait;
use heikas_application::configuration::{AgentDriverKind, EffectiveConfiguration};
use heikas_application::error::{ApplicationError, ApplicationResult};
use heikas_application::ports::agent::AgentDriver;
use heikas_application::ports::clock::Clock;
use heikas_application::ports::git::GitService;
use heikas_application::ports::observability::Redactor;
use heikas_application::ports::process::ProcessRunner;
use heikas_application::ports::quality::{ReviewProvider, TestGateRunner};
use heikas_application::ports::runtime::RuntimeFactory;

use crate::agent::{DeterministicFakeAgentDriver, ExternalCliAgentDriver, LocalModelAgentDriver};
use crate::quality::ai_review::AdvisoryAiReviewProvider;
use crate::quality::sonar::{SonarMcpProvider, SonarScannerProvider};
use crate::quality::{CommandTestGateRunner, LocalQualityProvider};
use crate::redaction::PatternRedactor;

pub struct AdapterRuntimeFactory {
    processes: Arc<dyn ProcessRunner>,
    git: Arc<dyn GitService>,
    clock: Arc<dyn Clock>,
}

impl AdapterRuntimeFactory {
    pub fn new(
        processes: Arc<dyn ProcessRunner>,
        git: Arc<dyn GitService>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            processes,
            git,
            clock,
        }
    }
}

#[async_trait]
impl RuntimeFactory for AdapterRuntimeFactory {
    async fn agent_driver(
        &self,
        configuration: &EffectiveConfiguration,
    ) -> ApplicationResult<Arc<dyn AgentDriver>> {
        match configuration.agent.driver {
            AgentDriverKind::Local => Ok(Arc::new(LocalModelAgentDriver::new(
                configuration.agent.clone(),
                Arc::clone(&self.processes),
            )?)),
            AgentDriverKind::Fake => {
                if !configuration.demonstration_mode {
                    return Err(ApplicationError::InvalidConfiguration(
                        "the deterministic demonstration agent may only run in demonstration mode"
                            .to_string(),
                    ));
                }
                let script = configuration.agent.fixture_script.clone().ok_or_else(|| {
                    ApplicationError::InvalidConfiguration(
                        "the demonstration agent requires a fixture script path".to_string(),
                    )
                })?;
                Ok(Arc::new(DeterministicFakeAgentDriver::load(&script)?))
            }
            kind => Ok(Arc::new(ExternalCliAgentDriver::new(
                kind,
                configuration.agent.clone(),
                Arc::clone(&self.processes),
            )?)),
        }
    }

    async fn review_providers(
        &self,
        configuration: &EffectiveConfiguration,
    ) -> ApplicationResult<Vec<Arc<dyn ReviewProvider>>> {
        let mut providers: Vec<Arc<dyn ReviewProvider>> =
            vec![Arc::new(LocalQualityProvider::new(
                Arc::clone(&self.processes),
                Arc::clone(&self.git),
                Arc::clone(&self.clock),
            ))];
        if configuration.quality.sonar_scanner.enabled {
            providers.push(Arc::new(SonarScannerProvider::new(
                configuration.quality.sonar_scanner.clone(),
                Arc::clone(&self.processes),
                Arc::clone(&self.clock),
            )));
        }
        if configuration.quality.sonar_mcp.enabled {
            providers.push(Arc::new(SonarMcpProvider::new(
                configuration.quality.sonar_mcp.clone(),
                Arc::clone(&self.processes),
                Arc::clone(&self.clock),
            )));
        }
        if configuration.quality.ai_review.enabled {
            let agent = self.agent_driver(configuration).await?;
            providers.push(Arc::new(AdvisoryAiReviewProvider::new(
                configuration.quality.ai_review.clone(),
                agent,
                Arc::clone(&self.clock),
            )));
        }
        Ok(providers)
    }

    async fn test_runner(
        &self,
        _configuration: &EffectiveConfiguration,
    ) -> ApplicationResult<Arc<dyn TestGateRunner>> {
        Ok(Arc::new(CommandTestGateRunner::new(Arc::clone(
            &self.processes,
        ))))
    }

    async fn redactor(
        &self,
        configuration: &EffectiveConfiguration,
    ) -> ApplicationResult<Arc<dyn Redactor>> {
        let home = if configuration.redaction.redact_home_prefix {
            std::env::var("HOME")
                .ok()
                .or_else(|| std::env::var("USERPROFILE").ok())
        } else {
            None
        };
        Ok(Arc::new(PatternRedactor::new(
            &configuration.redaction.secret_environment_variables,
            &configuration.redaction.additional_patterns,
            home,
        )))
    }
}
