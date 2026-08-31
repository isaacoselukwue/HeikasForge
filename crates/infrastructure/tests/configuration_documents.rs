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
        command_source: Default::default(),
        detection_notes: Vec::new(),
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

#[test]
fn every_field_the_renderer_emits_is_accepted_by_the_parser() {
    let specification = CommandSpecification {
        id: CommandId::from_str("node-test").expect("a command identifier"),
        kind: CommandKind::Test,
        program: "npm".to_string(),
        args: vec!["run".to_string(), "test".to_string()],
        working_subdirectory: Some("services/api".to_string()),
        timeout: TimeoutSeconds::clamped(1_800, MAXIMUM_COMMAND_TIMEOUT_SECONDS),
        required: true,
        report_format: ReportFormat::PytestText,
        report_path: None,
        environment: vec![("CI".to_string(), "1".to_string())],
        success_exit_codes: vec![0, 5],
    };
    let document = render_document(&configuration(
        vec![specification.clone()],
        vec![".git/**".to_string()],
    ));

    let parsed: toml::Value = toml::from_str(&document).expect("the document is valid TOML");
    let commands = parsed
        .get("commands")
        .and_then(|value| value.as_array())
        .expect("a commands array");
    let entry = commands.first().expect("one command");
    assert_eq!(
        entry.get("working_subdirectory").and_then(|v| v.as_str()),
        Some("services/api"),
        "a module subdirectory must survive being written and read back"
    );
    assert!(entry.get("success_exit_codes").is_some());
    assert!(entry.get("environment").is_some());
    assert_eq!(
        entry.get("report_format").and_then(|v| v.as_str()),
        Some("pytest_text"),
        "the report format decides whether an empty suite can be detected, so it must survive"
    );

    let round_tripped: heikas_infrastructure::configuration::document::ForgeDocument =
        toml::from_str(&document).expect("the renderer only emits keys the parser accepts");
    let sections = round_tripped.commands.expect("commands parse back");
    let section = sections.first().expect("one command section");
    assert_eq!(
        section.working_subdirectory.as_deref(),
        Some("services/api")
    );
    assert_eq!(
        section.success_exit_codes.clone().unwrap_or_default(),
        vec![0, 5]
    );
    assert_eq!(
        section.environment.clone().unwrap_or_default(),
        vec![("CI".to_string(), "1".to_string())]
    );
    assert_eq!(section.report_format.as_deref(), Some("pytest_text"));
}
