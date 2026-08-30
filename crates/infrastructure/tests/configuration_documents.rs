use std::path::PathBuf;
use std::str::FromStr;

use heikas_application::configuration::{
    AgentConfiguration, EffectiveConfiguration, GitConfiguration, QualityConfiguration,
    RedactionConfiguration, CONFIGURATION_SCHEMA_VERSION,
};
use heikas_domain::budget::RunBudgets;
use heikas_domain::clock::TimeoutSeconds;
use heikas_domain::command::{
    CommandCatalogue, CommandId, CommandKind, CommandSpecification, ReportFormat,
    MAXIMUM_COMMAND_TIMEOUT_SECONDS,
};
use heikas_domain::path_policy::PathPolicy;
use heikas_domain::retry::{NodeTimeouts, RetryPolicy};
use heikas_domain::run::CommitPolicy;
use heikas_infrastructure::configuration::resolver::render_document;

fn command(program: &str, args: &[&str], report_path: Option<&str>) -> CommandSpecification {
    CommandSpecification {
        id: CommandId::from_str("test").expect("a command identifier"),
        kind: CommandKind::Test,
        program: program.to_string(),
        args: args.iter().map(|value| (*value).to_string()).collect(),
        working_subdirectory: None,
        timeout: TimeoutSeconds::clamped(300, MAXIMUM_COMMAND_TIMEOUT_SECONDS),
        required: true,
        report_format: if report_path.is_some() {
            ReportFormat::JUnitXml
        } else {
            ReportFormat::None
        },
        report_path: report_path.map(str::to_string),
        environment: Vec::new(),
        success_exit_codes: vec![0],
    }
}

fn configuration(
    commands: Vec<CommandSpecification>,
    protected: Vec<String>,
) -> EffectiveConfiguration {
    EffectiveConfiguration {
        schema_version: CONFIGURATION_SCHEMA_VERSION,
        repository_path: PathBuf::from("/repositories/example"),
        budgets: RunBudgets::default(),
        commit_policy: CommitPolicy::Manual,
        agent: AgentConfiguration::default(),
        quality: QualityConfiguration::default(),
        git: GitConfiguration::default(),
        commands: CommandCatalogue { commands },
        path_policy: PathPolicy {
            protected_patterns: protected,
            ..PathPolicy::default()
        },
        redaction: RedactionConfiguration::default(),
        retry: RetryPolicy::default(),
        timeouts: NodeTimeouts::default(),
        environment_allowlist: Vec::new(),
        demonstration_mode: false,
        repository_trust: Default::default(),
    }
}

#[test]
fn a_rendered_document_is_valid_toml() {
    let document = render_document(&configuration(
        vec![command(
            "python3",
            &["scripts/gate.py", "test"],
            Some("reports/junit.xml"),
        )],
        vec![".git/**".to_string()],
    ));
    let parsed: toml::Value = toml::from_str(&document).expect("the rendered document parses");
    assert_eq!(parsed["schema_version"].as_integer(), Some(1));
    assert_eq!(parsed["commands"][0]["program"].as_str(), Some("python3"));
}

#[test]
fn a_windows_style_path_survives_the_round_trip() {
    let program = r"C:\Program Files\Python\python.exe";
    let report = r"reports\junit.xml";
    let protected = vec![r"C:\Users\operator\.ssh\**".to_string()];
    let document = render_document(&configuration(
        vec![command(program, &[r"C:\scripts\gate.py"], Some(report))],
        protected.clone(),
    ));

    let parsed: toml::Value =
        toml::from_str(&document).expect("a document containing Windows paths must parse");
    assert_eq!(parsed["commands"][0]["program"].as_str(), Some(program));
    assert_eq!(
        parsed["commands"][0]["args"][0].as_str(),
        Some(r"C:\scripts\gate.py")
    );
    assert_eq!(parsed["commands"][0]["report_path"].as_str(), Some(report));
    assert_eq!(
        parsed["policy"]["protected_paths"][0].as_str(),
        Some(protected[0].as_str())
    );
}

#[test]
fn a_value_containing_a_quote_or_control_character_is_escaped() {
    let program = "weird\"program\tname";
    let document = render_document(&configuration(
        vec![command(program, &["--flag=\"quoted\""], None)],
        vec!["pattern\"with\"quotes".to_string()],
    ));
    let parsed: toml::Value =
        toml::from_str(&document).expect("a document containing quotes must parse");
    assert_eq!(parsed["commands"][0]["program"].as_str(), Some(program));
    assert_eq!(
        parsed["commands"][0]["args"][0].as_str(),
        Some("--flag=\"quoted\"")
    );
    assert_eq!(
        parsed["policy"]["protected_paths"][0].as_str(),
        Some("pattern\"with\"quotes")
    );
}
