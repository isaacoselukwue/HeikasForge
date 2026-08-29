use heikas_domain::identity::ContentDigest;
use heikas_domain::run::CandidateStrategy;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::error::{ApplicationError, ApplicationResult};
use crate::ports::agent::{AgentRole, PromptContract};

pub const PROMPT_TEMPLATE_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PromptFacts {
    pub task_title: String,
    pub task_body: String,
    pub repository_summary: String,
    pub approved_plan_hash: Option<String>,
    pub approved_plan: Option<String>,
    pub strategy: Option<String>,
    pub strategy_emphasis: Option<String>,
    pub allowed_commands: Vec<String>,
    pub protected_paths: Vec<String>,
    pub previous_evidence: Vec<String>,
    pub expected_files: Vec<String>,
    pub attempt: u32,
}

const COMMON_CONSTRAINTS: &str = "Forbidden actions:\n\
- Do not run a general shell command. Only the named commands listed above are available.\n\
- Do not read, write or delete anything outside the assigned worktree.\n\
- Do not create a commit, push, fetch, merge, rebase, change a remote or alter Git configuration.\n\
- Do not weaken, delete, skip or disable an existing test, coverage threshold or quality rule.\n\
- Do not write source comments, commented-out code or task markers.\n\
- Do not place an em dash character in any file you create or edit.\n\
- Use British English in documentation, user-facing strings and messages.\n";

pub struct PromptLibrary;

impl PromptLibrary {
    pub fn render(role: AgentRole, facts: &PromptFacts) -> ApplicationResult<PromptContract> {
        let (template_id, body, schema) = match role {
            AgentRole::Planner => ("plan", Self::planner_body(facts), Self::planner_schema()),
            AgentRole::Implementer => (
                "implement",
                Self::implementer_body(facts)?,
                Self::implementer_schema(),
            ),
            AgentRole::Repairer => (
                "repair",
                Self::repairer_body(facts)?,
                Self::implementer_schema(),
            ),
            AgentRole::Reviewer => (
                "review",
                Self::reviewer_body(facts),
                Self::reviewer_schema(),
            ),
        };
        let template_hash = ContentDigest::of_str(&format!(
            "{template_id}:{PROMPT_TEMPLATE_VERSION}:{}",
            Self::template_source(role)
        ));
        Ok(PromptContract {
            template_id: template_id.to_string(),
            template_version: PROMPT_TEMPLATE_VERSION,
            template_hash,
            rendered: body,
            completion_schema: schema,
        })
    }

    fn template_source(role: AgentRole) -> &'static str {
        match role {
            AgentRole::Planner => PLANNER_TEMPLATE,
            AgentRole::Implementer => IMPLEMENTER_TEMPLATE,
            AgentRole::Repairer => REPAIRER_TEMPLATE,
            AgentRole::Reviewer => REVIEWER_TEMPLATE,
        }
    }

    fn planner_body(facts: &PromptFacts) -> String {
        let mut body = String::new();
        body.push_str(PLANNER_TEMPLATE);
        body.push_str("\n\nTask title:\n");
        body.push_str(&facts.task_title);
        body.push_str("\n\nTask description:\n");
        body.push_str(&facts.task_body);
        body.push_str("\n\nRepository facts:\n");
        body.push_str(&facts.repository_summary);
        body.push_str("\n\nProtected paths that must not be modified:\n");
        for path in &facts.protected_paths {
            body.push_str(&format!("- {path}\n"));
        }
        body.push_str("\nAvailable named commands:\n");
        for command in &facts.allowed_commands {
            body.push_str(&format!("- {command}\n"));
        }
        body.push_str("\nRequired plan headings, in this order:\n");
        for heading in heikas_domain::plan::REQUIRED_PLAN_HEADINGS {
            body.push_str(&format!("- {heading}\n"));
        }
        body.push('\n');
        body.push_str(COMMON_CONSTRAINTS);
        body
    }

    fn implementer_body(facts: &PromptFacts) -> ApplicationResult<String> {
        let plan = facts.approved_plan.as_ref().ok_or_else(|| {
            ApplicationError::Internal(
                "an implementation prompt requires an approved plan".to_string(),
            )
        })?;
        let plan_hash = facts.approved_plan_hash.as_ref().ok_or_else(|| {
            ApplicationError::Internal(
                "an implementation prompt requires the approved plan hash".to_string(),
            )
        })?;
        let mut body = String::new();
        body.push_str(IMPLEMENTER_TEMPLATE);
        body.push_str("\n\nApproved plan hash: ");
        body.push_str(plan_hash);
        if let (Some(strategy), Some(emphasis)) = (&facts.strategy, &facts.strategy_emphasis) {
            body.push_str(&format!(
                "\nCandidate strategy: {strategy}\nStrategy emphasis: {emphasis}"
            ));
        }
        body.push_str("\n\nTask title:\n");
        body.push_str(&facts.task_title);
        body.push_str("\n\nApproved plan:\n");
        body.push_str(plan);
        body.push_str("\n\nRepository facts:\n");
        body.push_str(&facts.repository_summary);
        body.push_str("\n\nFiles the plan expects to change:\n");
        for path in &facts.expected_files {
            body.push_str(&format!("- {path}\n"));
        }
        body.push_str("\nAvailable named commands:\n");
        for command in &facts.allowed_commands {
            body.push_str(&format!("- {command}\n"));
        }
        body.push_str("\nProtected paths that must not be modified:\n");
        for path in &facts.protected_paths {
            body.push_str(&format!("- {path}\n"));
        }
        body.push('\n');
        body.push_str(COMMON_CONSTRAINTS);
        Ok(body)
    }

    fn repairer_body(facts: &PromptFacts) -> ApplicationResult<String> {
        let plan = facts.approved_plan.as_ref().ok_or_else(|| {
            ApplicationError::Internal("a repair prompt requires an approved plan".to_string())
        })?;
        let mut body = String::new();
        body.push_str(REPAIRER_TEMPLATE);
        body.push_str(&format!("\n\nRepair attempt: {}", facts.attempt));
        if let Some(hash) = &facts.approved_plan_hash {
            body.push_str(&format!("\nApproved plan hash: {hash}"));
        }
        body.push_str("\n\nApproved plan:\n");
        body.push_str(plan);
        body.push_str("\n\nEvidence from the failing gates:\n");
        for line in &facts.previous_evidence {
            body.push_str(&format!("- {line}\n"));
        }
        body.push_str("\nAvailable named commands:\n");
        for command in &facts.allowed_commands {
            body.push_str(&format!("- {command}\n"));
        }
        body.push('\n');
        body.push_str(COMMON_CONSTRAINTS);
        Ok(body)
    }

    fn reviewer_body(facts: &PromptFacts) -> String {
        let mut body = String::new();
        body.push_str(REVIEWER_TEMPLATE);
        body.push_str("\n\nTask title:\n");
        body.push_str(&facts.task_title);
        body.push_str("\n\nEvidence available:\n");
        for line in &facts.previous_evidence {
            body.push_str(&format!("- {line}\n"));
        }
        body.push('\n');
        body.push_str(COMMON_CONSTRAINTS);
        body
    }

    fn planner_schema() -> serde_json::Value {
        json!({
            "type": "object",
            "required": ["plan_markdown", "expected_files", "summary"],
            "additionalProperties": false,
            "properties": {
                "plan_markdown": { "type": "string" },
                "expected_files": { "type": "array", "items": { "type": "string" } },
                "summary": { "type": "string" }
            }
        })
    }

    fn implementer_schema() -> serde_json::Value {
        json!({
            "type": "object",
            "required": ["summary", "changed_files", "tests_added"],
            "additionalProperties": false,
            "properties": {
                "summary": { "type": "string" },
                "changed_files": { "type": "array", "items": { "type": "string" } },
                "tests_added": { "type": "array", "items": { "type": "string" } },
                "remaining_risks": { "type": "array", "items": { "type": "string" } }
            }
        })
    }

    fn reviewer_schema() -> serde_json::Value {
        json!({
            "type": "object",
            "required": ["findings"],
            "additionalProperties": false,
            "properties": {
                "findings": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "required": ["rule_id", "severity", "category", "message"],
                        "additionalProperties": false,
                        "properties": {
                            "rule_id": { "type": "string" },
                            "severity": { "type": "string" },
                            "category": { "type": "string" },
                            "message": { "type": "string" },
                            "file": { "type": "string" },
                            "line": { "type": "integer" }
                        }
                    }
                }
            }
        })
    }
}

pub fn strategy_facts(strategy: CandidateStrategy) -> (String, String) {
    (
        strategy.as_str().to_string(),
        strategy.emphasis().to_string(),
    )
}

const PLANNER_TEMPLATE: &str = "You are the planning node of a local-first engineering orchestrator.\n\
Your goal is to produce a precise, evidence-based implementation plan for the task below.\n\
You have read-only tools. You may list directories, read files, search the repository and inspect Git status.\n\
You may not change a single file during planning.\n\
Ground every statement in observed repository evidence. Label anything you cannot verify as an assumption.\n\
Return the complete plan through the structured completion tool using the exact schema provided.";

const IMPLEMENTER_TEMPLATE: &str = "You are the implementation node of a local-first engineering orchestrator.\n\
Implement the approved plan inside your assigned candidate worktree.\n\
Preserve existing public behaviour unless the task changes it. Add or update tests for the behaviour you introduce.\n\
Avoid unrelated refactoring. Leave the worktree in a state where the configured test commands can run.\n\
Your work is judged by the resulting Git diff and by the configured gates, not by your summary.\n\
Return the structured completion once the change is complete.";

const REPAIRER_TEMPLATE: &str = "You are the repair node of a local-first engineering orchestrator.\n\
The configured gates rejected the current candidate. Repair the candidate worktree so that every required gate passes.\n\
Address the reported failures directly. Do not delete or weaken a test to make a gate pass.\n\
Keep the change consistent with the approved plan.\n\
Return the structured completion once the repair is complete.";

const REVIEWER_TEMPLATE: &str = "You are an advisory review node of a local-first engineering orchestrator.\n\
You have read-only tools. Report maintainability and design observations for the change under review.\n\
Your findings are advisory. They cannot override a deterministic test or scanner result.\n\
Return the structured completion with your findings.";
