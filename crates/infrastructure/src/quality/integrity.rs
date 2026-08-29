use std::path::Path;
use std::sync::Arc;

use heikas_application::error::ApplicationResult;
use heikas_application::ports::git::GitService;
use heikas_application::ports::quality::GateContext;
use heikas_domain::review::{IssueCategory, IssueSeverity, ReviewIssue};

pub const PROVIDER: &str = "test-integrity";

const TEST_FUNCTION_MARKERS: [&str; 10] = [
    "#[test]",
    "#[tokio::test]",
    "#[rstest]",
    "#[test_case",
    "def test_",
    "func Test",
    "it(",
    "test(",
    "describe(",
    "@Test",
];

const SKIP_MARKERS: [&str; 12] = [
    "#[ignore]",
    "#[ignore =",
    "@pytest.mark.skip",
    "pytest.skip(",
    "@unittest.skip",
    "it.skip(",
    "test.skip(",
    "describe.skip(",
    "xit(",
    "xdescribe(",
    "t.Skip(",
    "@Disabled",
];

const ASSERTION_MARKERS: [&str; 12] = [
    "assert!",
    "assert_eq!",
    "assert_ne!",
    "debug_assert",
    "assert ",
    "assertEqual",
    "expect(",
    "assert.",
    "t.Error",
    "t.Fatal",
    "should.",
    "verify(",
];

const QUALITY_CONFIGURATION_FILES: [&str; 20] = [
    "clippy.toml",
    ".clippy.toml",
    "rustfmt.toml",
    ".rustfmt.toml",
    "tarpaulin.toml",
    ".eslintrc",
    ".eslintrc.json",
    ".eslintrc.cjs",
    "eslint.config.js",
    "eslint.config.mjs",
    ".prettierrc",
    "tsconfig.json",
    "pyproject.toml",
    "setup.cfg",
    ".flake8",
    "jest.config.js",
    "vitest.config.ts",
    "sonar-project.properties",
    "codecov.yml",
    ".heikas/forge.toml",
];

const COVERAGE_KEYS: [&str; 8] = [
    "minimum_line_coverage",
    "fail_under",
    "coverageThreshold",
    "lines",
    "statements",
    "branches",
    "functions",
    "min_coverage",
];

pub async fn evaluate(
    context: &GateContext,
    git: &Arc<dyn GitService>,
) -> ApplicationResult<Vec<ReviewIssue>> {
    if !context.configuration.quality.protect_existing_tests {
        return Ok(Vec::new());
    }
    let mut issues = Vec::new();
    for path in &context.changed_paths {
        let approved = context
            .plan_expected_files
            .iter()
            .any(|expected| expected.trim_matches('`') == path);
        let baseline = git
            .file_at_commit(&context.repository, &context.baseline, path)
            .await?;
        let Some(baseline_bytes) = baseline else {
            continue;
        };
        let Ok(baseline_text) = String::from_utf8(baseline_bytes) else {
            continue;
        };
        let current_text = read_current(&context.worktree, path);

        if is_quality_configuration(path) {
            if let Some(current) = &current_text {
                issues.extend(coverage_threshold_issues(
                    path,
                    &baseline_text,
                    current,
                    approved,
                ));
            } else {
                issues.push(issue(
                    path,
                    "quality-configuration-deleted",
                    IssueSeverity::Blocker,
                    format!("the quality configuration `{path}` was deleted"),
                    approved,
                ));
            }
        }

        if !is_test_path(path) {
            continue;
        }

        let Some(current) = current_text else {
            issues.push(issue(
                path,
                "existing-test-file-deleted",
                IssueSeverity::Blocker,
                format!("the existing test file `{path}` was deleted"),
                approved,
            ));
            continue;
        };

        let baseline_tests = count_markers(&baseline_text, &TEST_FUNCTION_MARKERS);
        let current_tests = count_markers(&current, &TEST_FUNCTION_MARKERS);
        if current_tests < baseline_tests {
            issues.push(issue(
                path,
                "existing-test-removed",
                IssueSeverity::Blocker,
                format!(
                    "`{path}` declared {baseline_tests} tests at the baseline but now declares {current_tests}"
                ),
                approved,
            ));
        }

        let baseline_skips = count_markers(&baseline_text, &SKIP_MARKERS);
        let current_skips = count_markers(&current, &SKIP_MARKERS);
        if current_skips > baseline_skips {
            issues.push(issue(
                path,
                "test-skip-marker-added",
                IssueSeverity::Blocker,
                format!(
                    "`{path}` added {} skip markers relative to the baseline",
                    current_skips - baseline_skips
                ),
                approved,
            ));
        }

        let baseline_assertions = count_markers(&baseline_text, &ASSERTION_MARKERS);
        let current_assertions = count_markers(&current, &ASSERTION_MARKERS);
        if current_assertions + 1 < baseline_assertions {
            issues.push(issue(
                path,
                "test-assertions-weakened",
                IssueSeverity::High,
                format!(
                    "`{path}` fell from {baseline_assertions} assertions to {current_assertions}"
                ),
                approved,
            ));
        }
    }
    Ok(issues)
}

fn read_current(worktree: &Path, relative: &str) -> Option<String> {
    std::fs::read_to_string(worktree.join(relative)).ok()
}

pub fn is_test_path(path: &str) -> bool {
    let lowered = path.to_ascii_lowercase();
    let file_name = lowered.rsplit('/').next().unwrap_or(&lowered).to_string();
    lowered.split('/').any(|segment| {
        segment == "tests" || segment == "test" || segment == "spec" || segment == "__tests__"
    }) || file_name.starts_with("test_")
        || file_name.contains("_test.")
        || file_name.contains(".test.")
        || file_name.contains(".spec.")
        || file_name.contains("_spec.")
}

fn is_quality_configuration(path: &str) -> bool {
    let lowered = path.to_ascii_lowercase();
    QUALITY_CONFIGURATION_FILES
        .iter()
        .any(|candidate| lowered == *candidate || lowered.ends_with(&format!("/{candidate}")))
}

fn count_markers(text: &str, markers: &[&str]) -> usize {
    markers
        .iter()
        .map(|marker| text.matches(marker).count())
        .sum()
}

fn coverage_threshold_issues(
    path: &str,
    baseline: &str,
    current: &str,
    approved: bool,
) -> Vec<ReviewIssue> {
    let mut issues = Vec::new();
    for key in COVERAGE_KEYS {
        let baseline_value = extract_numeric(baseline, key);
        let current_value = extract_numeric(current, key);
        if let (Some(before), Some(after)) = (baseline_value, current_value) {
            if after + f64::EPSILON < before {
                issues.push(issue(
                    path,
                    "coverage-threshold-reduced",
                    IssueSeverity::Blocker,
                    format!("`{path}` reduced `{key}` from {before} to {after}"),
                    approved,
                ));
            }
        }
    }
    issues
}

fn extract_numeric(text: &str, key: &str) -> Option<f64> {
    let index = text.find(key)?;
    let tail = &text[index + key.len()..];
    let mut digits = String::new();
    let mut seen_separator = false;
    for character in tail.chars() {
        if character.is_ascii_digit() || (character == '.' && !digits.is_empty()) {
            digits.push(character);
        } else if digits.is_empty() {
            if matches!(character, ':' | '=' | ' ' | '"' | '\'' | '\t') {
                seen_separator = true;
                continue;
            }
            if seen_separator {
                return None;
            }
            return None;
        } else {
            break;
        }
    }
    digits.parse::<f64>().ok()
}

fn issue(
    path: &str,
    rule_id: &str,
    severity: IssueSeverity,
    message: String,
    approved: bool,
) -> ReviewIssue {
    let effective = if approved {
        IssueSeverity::Medium
    } else {
        severity
    };
    let final_message = if approved {
        format!("{message}. The approved plan names this path, so the finding is advisory.")
    } else {
        message
    };
    ReviewIssue {
        provider: PROVIDER.to_string(),
        fingerprint: ReviewIssue::compute_fingerprint(
            PROVIDER,
            rule_id,
            Some(path),
            &final_message,
        ),
        rule_id: rule_id.to_string(),
        category: IssueCategory::TestIntegrity,
        severity: effective,
        file: Some(path.to_string()),
        line: None,
        column: None,
        message: final_message,
        help_reference: None,
        is_new: true,
    }
}
