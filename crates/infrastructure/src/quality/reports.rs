use heikas_application::error::ApplicationResult;
use heikas_domain::review::{IssueCategory, IssueSeverity, ReviewIssue};
use heikas_domain::test_evidence::TestFailureDetail;
use quick_xml::events::Event;
use quick_xml::Reader;
use serde::Deserialize;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TestSummary {
    pub total: u32,
    pub failed: u32,
    pub skipped: u32,
    pub failures: Vec<TestFailureDetail>,
}

pub fn parse_junit_xml(contents: &str) -> ApplicationResult<TestSummary> {
    let mut reader = Reader::from_str(contents);
    reader.config_mut().trim_text(true);
    let mut summary = TestSummary::default();
    let mut current_suite = String::new();
    let mut current_case = String::new();
    let mut current_file: Option<String> = None;
    let mut current_line: Option<u32> = None;
    let mut pending_failure: Option<String> = None;
    let mut buffer = Vec::new();

    loop {
        match reader.read_event_into(&mut buffer) {
            Ok(Event::Start(element)) | Ok(Event::Empty(element)) => {
                let name = String::from_utf8_lossy(element.name().as_ref()).to_string();
                let attributes = collect_attributes(&element);
                match name.as_str() {
                    "testsuite" => {
                        current_suite = attributes
                            .iter()
                            .find(|(key, _)| key == "name")
                            .map(|(_, value)| value.clone())
                            .unwrap_or_default();
                    }
                    "testcase" => {
                        current_case = attributes
                            .iter()
                            .find(|(key, _)| key == "name")
                            .map(|(_, value)| value.clone())
                            .unwrap_or_default();
                        let classname = attributes
                            .iter()
                            .find(|(key, _)| key == "classname")
                            .map(|(_, value)| value.clone());
                        if let Some(classname) = classname {
                            if !classname.is_empty() {
                                current_suite = classname;
                            }
                        }
                        current_file = attributes
                            .iter()
                            .find(|(key, _)| key == "file")
                            .map(|(_, value)| value.clone());
                        current_line = attributes
                            .iter()
                            .find(|(key, _)| key == "line")
                            .and_then(|(_, value)| value.parse::<u32>().ok());
                        summary.total += 1;
                    }
                    "failure" | "error" => {
                        summary.failed += 1;
                        let message = attributes
                            .iter()
                            .find(|(key, _)| key == "message")
                            .map(|(_, value)| value.clone())
                            .unwrap_or_else(|| "the test failed".to_string());
                        pending_failure = Some(message.clone());
                        summary.failures.push(TestFailureDetail {
                            suite: current_suite.clone(),
                            case: current_case.clone(),
                            message,
                            file: current_file.clone(),
                            line: current_line,
                        });
                    }
                    "skipped" => summary.skipped += 1,
                    _ => {}
                }
                if matches!(name.as_str(), "failure" | "error")
                    && matches!(reader.read_event_into(&mut Vec::new()), Ok(Event::Text(_)))
                {
                    pending_failure = None;
                }
            }
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(_) => break,
        }
        buffer.clear();
    }
    let _ = pending_failure;
    Ok(summary)
}

fn collect_attributes(element: &quick_xml::events::BytesStart<'_>) -> Vec<(String, String)> {
    element
        .attributes()
        .filter_map(Result::ok)
        .map(|attribute| {
            (
                String::from_utf8_lossy(attribute.key.as_ref()).to_string(),
                attribute
                    .normalized_value(quick_xml::XmlVersion::Implicit1_0)
                    .map(|value| value.to_string())
                    .unwrap_or_default(),
            )
        })
        .collect()
}

pub fn parse_lcov_coverage(contents: &str) -> Option<f64> {
    let mut found = 0u64;
    let mut hit = 0u64;
    for line in contents.lines() {
        if let Some(value) = line.strip_prefix("LF:") {
            found += value.trim().parse::<u64>().unwrap_or(0);
        } else if let Some(value) = line.strip_prefix("LH:") {
            hit += value.trim().parse::<u64>().unwrap_or(0);
        }
    }
    if found == 0 {
        return None;
    }
    Some((hit as f64 / found as f64) * 100.0)
}

#[derive(Debug, Deserialize)]
struct CargoTestEvent {
    #[serde(rename = "type")]
    kind: Option<String>,
    event: Option<String>,
    name: Option<String>,
    stdout: Option<String>,
    #[serde(default)]
    passed: Option<u32>,
    #[serde(default)]
    failed: Option<u32>,
    #[serde(default)]
    ignored: Option<u32>,
}

pub fn parse_cargo_test_json(contents: &str) -> TestSummary {
    let mut summary = TestSummary::default();
    for line in contents.lines() {
        let Ok(event) = serde_json::from_str::<CargoTestEvent>(line) else {
            continue;
        };
        match (event.kind.as_deref(), event.event.as_deref()) {
            (Some("test"), Some("ok")) => summary.total += 1,
            (Some("test"), Some("ignored")) => {
                summary.total += 1;
                summary.skipped += 1;
            }
            (Some("test"), Some("failed")) => {
                summary.total += 1;
                summary.failed += 1;
                let name = event.name.clone().unwrap_or_else(|| "unknown".to_string());
                let (suite, case) = split_test_name(&name);
                summary.failures.push(TestFailureDetail {
                    suite,
                    case,
                    message: event
                        .stdout
                        .unwrap_or_else(|| "the test failed".to_string())
                        .chars()
                        .take(4_000)
                        .collect(),
                    file: None,
                    line: None,
                });
            }
            (Some("suite"), Some("ok")) | (Some("suite"), Some("failed")) if summary.total == 0 => {
                summary.total = event.passed.unwrap_or(0)
                    + event.failed.unwrap_or(0)
                    + event.ignored.unwrap_or(0);
                summary.failed = event.failed.unwrap_or(0);
                summary.skipped = event.ignored.unwrap_or(0);
            }
            _ => {}
        }
    }
    summary
}

fn split_test_name(name: &str) -> (String, String) {
    match name.rsplit_once("::") {
        Some((suite, case)) => (suite.to_string(), case.to_string()),
        None => ("tests".to_string(), name.to_string()),
    }
}

#[derive(Debug, Deserialize)]
struct SarifDocument {
    #[serde(default)]
    runs: Vec<SarifRun>,
}

#[derive(Debug, Deserialize)]
struct SarifRun {
    #[serde(default)]
    results: Vec<SarifResult>,
    #[serde(default)]
    tool: Option<SarifTool>,
}

#[derive(Debug, Deserialize)]
struct SarifTool {
    #[serde(default)]
    driver: Option<SarifDriver>,
}

#[derive(Debug, Deserialize)]
struct SarifDriver {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    rules: Vec<SarifRule>,
}

#[derive(Debug, Deserialize)]
struct SarifRule {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    #[serde(rename = "helpUri")]
    help_uri: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SarifResult {
    #[serde(default)]
    #[serde(rename = "ruleId")]
    rule_id: Option<String>,
    #[serde(default)]
    level: Option<String>,
    #[serde(default)]
    message: Option<SarifMessage>,
    #[serde(default)]
    locations: Vec<SarifLocation>,
}

#[derive(Debug, Deserialize)]
struct SarifMessage {
    #[serde(default)]
    text: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SarifLocation {
    #[serde(default)]
    #[serde(rename = "physicalLocation")]
    physical_location: Option<SarifPhysicalLocation>,
}

#[derive(Debug, Deserialize)]
struct SarifPhysicalLocation {
    #[serde(default)]
    #[serde(rename = "artifactLocation")]
    artifact_location: Option<SarifArtifactLocation>,
    #[serde(default)]
    region: Option<SarifRegion>,
}

#[derive(Debug, Deserialize)]
struct SarifArtifactLocation {
    #[serde(default)]
    uri: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SarifRegion {
    #[serde(default)]
    #[serde(rename = "startLine")]
    start_line: Option<u32>,
    #[serde(default)]
    #[serde(rename = "startColumn")]
    start_column: Option<u32>,
}

pub fn parse_sarif(contents: &str, provider: &str) -> ApplicationResult<Vec<ReviewIssue>> {
    let document: SarifDocument = serde_json::from_str(contents)?;
    let mut issues = Vec::new();
    for run in document.runs {
        let tool_name = run
            .tool
            .as_ref()
            .and_then(|tool| tool.driver.as_ref())
            .and_then(|driver| driver.name.clone())
            .unwrap_or_else(|| provider.to_string());
        let help_uris: Vec<(String, Option<String>)> = run
            .tool
            .as_ref()
            .and_then(|tool| tool.driver.as_ref())
            .map(|driver| {
                driver
                    .rules
                    .iter()
                    .map(|rule| (rule.id.clone().unwrap_or_default(), rule.help_uri.clone()))
                    .collect()
            })
            .unwrap_or_default();
        for result in run.results {
            let rule_id = result.rule_id.unwrap_or_else(|| "unknown".to_string());
            let severity = match result.level.as_deref() {
                Some("error") => IssueSeverity::High,
                Some("warning") => IssueSeverity::Medium,
                Some("note") => IssueSeverity::Low,
                Some("none") => IssueSeverity::Info,
                _ => IssueSeverity::Medium,
            };
            let message = result
                .message
                .and_then(|message| message.text)
                .unwrap_or_else(|| "an analysis rule reported a finding".to_string());
            let location = result.locations.first().and_then(|location| {
                location.physical_location.as_ref().map(|physical| {
                    (
                        physical
                            .artifact_location
                            .as_ref()
                            .and_then(|artifact| artifact.uri.clone()),
                        physical
                            .region
                            .as_ref()
                            .and_then(|region| region.start_line),
                        physical
                            .region
                            .as_ref()
                            .and_then(|region| region.start_column),
                    )
                })
            });
            let (file, line, column) = location.unwrap_or((None, None, None));
            let help_reference = help_uris
                .iter()
                .find(|(id, _)| id == &rule_id)
                .and_then(|(_, uri)| uri.clone());
            issues.push(ReviewIssue {
                provider: tool_name.clone(),
                fingerprint: ReviewIssue::compute_fingerprint(
                    &tool_name,
                    &rule_id,
                    file.as_deref(),
                    &message,
                ),
                rule_id,
                category: IssueCategory::Security,
                severity,
                file,
                line,
                column,
                message,
                help_reference,
                is_new: true,
            });
        }
    }
    Ok(issues)
}

pub fn parse_cargo_test_summary(contents: &str) -> TestSummary {
    let mut summary = TestSummary::default();
    for line in contents.lines() {
        let Some(rest) = line.trim().strip_prefix("test result:") else {
            continue;
        };
        let Some((_, counts)) = rest.split_once('.') else {
            continue;
        };
        for field in counts.split(';') {
            let mut parts = field.split_whitespace();
            let (Some(value), Some(label)) = (parts.next(), parts.next()) else {
                continue;
            };
            let Ok(count) = value.parse::<u32>() else {
                continue;
            };
            match label {
                "passed" => summary.total += count,
                "failed" => {
                    summary.total += count;
                    summary.failed += count;
                }
                "ignored" => {
                    summary.total += count;
                    summary.skipped += count;
                }
                _ => {}
            }
        }
    }
    summary
}

pub fn parse_pytest_summary(contents: &str) -> TestSummary {
    let mut summary = TestSummary::default();
    for line in contents.lines().rev() {
        let trimmed = line.trim().trim_matches('=').trim();
        if trimmed.starts_with("no tests ran") {
            return TestSummary::default();
        }
        if !trimmed.contains(" in ") {
            continue;
        }
        let mut recognised = false;
        for field in trimmed.split(',') {
            let field = field.trim();
            let head = field.split(" in ").next().unwrap_or(field).trim();
            let mut parts = head.split_whitespace();
            let (Some(value), Some(label)) = (parts.next(), parts.next()) else {
                continue;
            };
            let Ok(count) = value.parse::<u32>() else {
                continue;
            };
            match label {
                "passed" | "xpassed" => {
                    summary.total += count;
                    recognised = true;
                }
                "failed" | "error" | "errors" => {
                    summary.total += count;
                    summary.failed += count;
                    recognised = true;
                }
                "skipped" | "deselected" | "xfailed" => {
                    summary.total += count;
                    summary.skipped += count;
                    recognised = true;
                }
                _ => {}
            }
        }
        if recognised {
            return summary;
        }
        summary = TestSummary::default();
    }
    summary
}

#[derive(Debug, Deserialize)]
struct GoTestEvent {
    #[serde(rename = "Action")]
    action: Option<String>,
    #[serde(rename = "Test")]
    test: Option<String>,
    #[serde(rename = "Package")]
    package: Option<String>,
    #[serde(rename = "Output")]
    output: Option<String>,
}

pub fn parse_go_test_json(contents: &str) -> TestSummary {
    let mut summary = TestSummary::default();
    let mut failure_output: std::collections::BTreeMap<String, String> =
        std::collections::BTreeMap::new();
    for line in contents.lines() {
        let Ok(event) = serde_json::from_str::<GoTestEvent>(line) else {
            continue;
        };
        let Some(test) = event.test.clone() else {
            continue;
        };
        let key = format!("{}::{test}", event.package.clone().unwrap_or_default());
        if event.action.as_deref() == Some("output") {
            if let Some(output) = event.output {
                let entry = failure_output.entry(key).or_default();
                if entry.len() < 4_000 {
                    entry.push_str(&output);
                }
            }
            continue;
        }
        match event.action.as_deref() {
            Some("pass") => summary.total += 1,
            Some("skip") => {
                summary.total += 1;
                summary.skipped += 1;
            }
            Some("fail") => {
                summary.total += 1;
                summary.failed += 1;
                let package = event.package.clone().unwrap_or_default();
                summary.failures.push(TestFailureDetail {
                    suite: package,
                    case: test,
                    message: failure_output
                        .get(&key)
                        .cloned()
                        .unwrap_or_else(|| "the test failed".to_string())
                        .chars()
                        .take(4_000)
                        .collect(),
                    file: None,
                    line: None,
                });
            }
            _ => {}
        }
    }
    summary
}

fn leading_count(text: &str) -> Option<u32> {
    text.split_whitespace().next()?.parse::<u32>().ok()
}

pub fn parse_node_test_summary(contents: &str) -> TestSummary {
    let mut summary = TestSummary::default();
    for line in contents.lines() {
        let trimmed = line.trim();

        if let Some(rest) = trimmed.strip_prefix("# ") {
            let mut parts = rest.split_whitespace();
            if let (Some(label), Some(value)) = (parts.next(), parts.next()) {
                if let Ok(count) = value.parse::<u32>() {
                    match label {
                        "pass" => summary.total += count,
                        "fail" => {
                            summary.total += count;
                            summary.failed += count;
                        }
                        "skipped" | "todo" | "cancelled" => {
                            summary.total += count;
                            summary.skipped += count;
                        }
                        _ => {}
                    }
                }
            }
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix("Tests:") {
            for field in rest.split(',') {
                let field = field.trim();
                let Some(count) = leading_count(field) else {
                    continue;
                };
                if field.ends_with("passed") {
                    summary.total += count;
                } else if field.ends_with("failed") {
                    summary.total += count;
                    summary.failed += count;
                } else if field.ends_with("skipped") || field.ends_with("todo") {
                    summary.total += count;
                    summary.skipped += count;
                }
            }
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix("Tests ") {
            let head = rest.split('(').next().unwrap_or(rest);
            for field in head.split('|') {
                let field = field.trim();
                let Some(count) = leading_count(field) else {
                    continue;
                };
                if field.ends_with("passed") {
                    summary.total += count;
                } else if field.ends_with("failed") {
                    summary.total += count;
                    summary.failed += count;
                } else if field.ends_with("skipped") || field.ends_with("todo") {
                    summary.total += count;
                    summary.skipped += count;
                }
            }
            continue;
        }

        if trimmed.ends_with("passing") || trimmed.contains("passing (") {
            if let Some(count) = leading_count(trimmed) {
                summary.total += count;
            }
            continue;
        }
        if trimmed.ends_with("pending") {
            if let Some(count) = leading_count(trimmed) {
                summary.total += count;
                summary.skipped += count;
            }
            continue;
        }
        if trimmed.ends_with("failing") {
            if let Some(count) = leading_count(trimmed) {
                summary.total += count;
                summary.failed += count;
            }
        }
    }
    summary
}

pub fn parse_ctest_summary(contents: &str) -> TestSummary {
    let mut summary = TestSummary::default();
    for line in contents.lines() {
        let trimmed = line.trim();
        let Some(index) = trimmed.find("tests passed,") else {
            continue;
        };
        let rest = &trimmed[index + "tests passed,".len()..];
        let mut parts = rest.split_whitespace();
        let Some(failed) = parts.next().and_then(|value| value.parse::<u32>().ok()) else {
            continue;
        };
        let total = rest
            .split("out of")
            .nth(1)
            .and_then(leading_count)
            .unwrap_or(failed);
        summary.total = total;
        summary.failed = failed;
        return summary;
    }
    summary
}
