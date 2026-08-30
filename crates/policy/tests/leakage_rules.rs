use std::path::Path;
use std::process::Command;

use heikas_policy::repository::TrackedRepository;
use heikas_policy::rules::internal_readme::PRIVATE_DOCUMENT_RULE;
use heikas_policy::rules::leakage::{host_account, secret_shape, HOST_PATH_RULE, SECRET_RULE};
use tempfile::TempDir;

fn git(directory: &Path, arguments: &[&str]) {
    let status = Command::new("git")
        .args(arguments)
        .current_dir(directory)
        .env("GIT_AUTHOR_NAME", "Isaac Oselukwue")
        .env("GIT_AUTHOR_EMAIL", "fixture@localhost.invalid")
        .env("GIT_COMMITTER_NAME", "Isaac Oselukwue")
        .env("GIT_COMMITTER_EMAIL", "fixture@localhost.invalid")
        .status()
        .expect("git runs");
    assert!(status.success(), "git {arguments:?} failed");
}

fn repository_with(files: &[(&str, &str)]) -> (TempDir, TrackedRepository) {
    let directory = TempDir::new().expect("a temporary repository");
    git(directory.path(), &["init", "--quiet"]);
    for (name, contents) in files {
        let path = directory.path().join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("the directory creates");
        }
        std::fs::write(&path, contents).expect("the file writes");
    }
    git(directory.path(), &["add", "-A"]);
    git(
        directory.path(),
        &["commit", "--quiet", "--message", "seed"],
    );
    let repository = TrackedRepository::discover(directory.path()).expect("the repository reads");
    (directory, repository)
}

#[test]
fn a_committed_runtime_descriptor_is_a_violation() {
    let (_directory, repository) = repository_with(&[(
        "apps/web/tests/.runtime.json",
        concat!(
            "{\n",
            "  \"bootstrapUrl\": \"http://127.0.0.1:33749/#token=bfee6b3f8c43530ece55db2b3dbcaa902d90905105617d744582f4e0cdeda703\",\n",
            "  \"heikasHome\": \"/home/ajay/Codes/HeikasForge/target/demonstration/home\",\n",
            "  \"serverPid\": 12345\n",
            "}\n"
        ),
    )]);
    let findings = heikas_policy::rules::leakage::check(&repository).expect("the rule runs");
    assert!(
        findings.iter().any(|finding| finding.rule == SECRET_RULE),
        "a committed session token must be reported: {findings:?}"
    );
    assert!(
        findings
            .iter()
            .any(|finding| finding.rule == HOST_PATH_RULE),
        "a committed host path must be reported: {findings:?}"
    );
}

#[test]
fn every_covered_secret_shape_is_recognised() {
    for line in [
        "token=bfee6b3f8c43530ece55db2b3dbcaa902d90905105617d744582f4e0cdeda703",
        "key: ghp_A9fQ2xLmZ0pR7tYw3BvNcEjHkSdUgT4iOa",
        "ANTHROPIC_API_KEY=sk-ant-A9fQ2xLmZ0pR7tYw3BvNcEjHkSdUgT4iOa",
        "OPENAI_API_KEY=sk-A9fQ2xLmZ0pR7tYw3BvNcEjHkSdUgT",
        "aws_access_key_id = AKIA7QW3ZP2LNVR6XCTD",
        "slack = xoxb-A9fQ2xLmZ0pR7tYw3BvNcEjHkSdUgT4iOa",
        "authorization: eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiI5ZjJhMWM4ZCJ9.dQw4W9XcZk2Lp7Rt3Bv",
    ] {
        assert!(
            secret_shape(line).is_some(),
            "`{line}` must be recognised as secret material"
        );
    }
}

#[test]
fn a_pattern_definition_and_a_documented_sample_are_not_reported() {
    for line in [
        "        r\"AKIA[0-9A-Z]{16}\",",
        "        re.compile(r\"-----BEGIN [A-Z ]*PRIVATE KEY-----\"),",
        "        \"AKIAIOSFODNN7EXAMPLE\",",
        "        \"ghp_0123456789abcdefghijklmnopqrstuvwxyz\",",
        "Set the token with `export SONAR_TOKEN=<your token>`.",
    ] {
        assert_eq!(
            secret_shape(line),
            None,
            "`{line}` describes a pattern or a documented sample and must not be reported"
        );
    }
}

#[test]
fn a_placeholder_home_directory_is_permitted_but_a_real_account_is_not() {
    assert_eq!(host_account("/home/you/projects/app"), None);
    assert_eq!(host_account("/home/runner/work/repo"), None);
    assert_eq!(
        host_account("/home/ajay/Codes/HeikasForge"),
        Some("ajay".to_string())
    );
    assert_eq!(
        host_account("C:\\Users\\Isaac\\Codes"),
        Some("Isaac".to_string())
    );
}

#[test]
fn tracking_a_private_working_document_is_a_violation() {
    let (_directory, repository) = repository_with(&[
        ("spec.md", "# Specification\n"),
        ("CLAUDE.md", "# Instructions\n"),
    ]);
    let findings =
        heikas_policy::rules::internal_readme::check(&repository).expect("the rule runs");
    let reported: Vec<&str> = findings
        .iter()
        .filter(|finding| finding.rule == PRIVATE_DOCUMENT_RULE)
        .filter_map(|finding| finding.path.as_deref())
        .collect();
    assert!(
        reported.contains(&"spec.md") && reported.contains(&"CLAUDE.md"),
        "both private working documents must be reported: {findings:?}"
    );
}

#[test]
fn a_repository_without_the_private_documents_passes() {
    let (_directory, repository) = repository_with(&[("README.md", "# Public\n")]);
    let findings =
        heikas_policy::rules::internal_readme::check(&repository).expect("the rule runs");
    assert!(
        findings.is_empty(),
        "a clean repository must report nothing: {findings:?}"
    );
}
