use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use heikas_application::configuration::EffectiveConfiguration;
use heikas_application::engine::DispatchOutcome;
use heikas_application::model::request::CreateRunRequest;
use heikas_application::usecases::ApplicationService;
use heikas_domain::identity::RunId;
use heikas_domain::state::RunProjection;
use heikas_infrastructure::layout::StoreLayout;
use heikas_infrastructure::{build_runtime, Runtime};
use serde_json::{json, Value};
use tempfile::TempDir;

pub const AUTHOR: &str = "Isaac Oselukwue";
pub const EMAIL: &str = "fixture@localhost.invalid";

pub fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .expect("the workspace root resolves")
}

pub struct Scenario {
    pub home: TempDir,
    pub repository_root: TempDir,
    pub repository: PathBuf,
    pub script: PathBuf,
    pub runtime: Runtime,
}

impl Scenario {
    pub fn service(&self) -> Arc<ApplicationService> {
        Arc::clone(&self.runtime.service)
    }

    pub async fn projection(&self, run: RunId) -> RunProjection {
        self.service()
            .projection(run)
            .await
            .expect("the projection loads")
    }

    pub async fn create_run(&self, candidates: u8) -> RunId {
        let mut request = CreateRunRequest::new(
            self.repository.clone(),
            std::fs::read_to_string(self.repository.join("TASK.md")).expect("the task reads"),
        );
        request.candidate_count = Some(candidates);
        request.max_parallel_candidates = Some(candidates);
        request.demonstration_mode = true;
        request.agent_driver = Some("fake".to_string());
        self.service()
            .create_run(request)
            .await
            .expect("the run is created")
    }

    pub async fn dispatch(&self, run: RunId) -> DispatchOutcome {
        self.service()
            .dispatch(run)
            .await
            .expect("the dispatch runs")
    }

    pub fn script_path(&self) -> &Path {
        &self.script
    }

    pub async fn configuration(&self, run: RunId) -> EffectiveConfiguration {
        self.runtime
            .store
            .configuration(run)
            .await
            .expect("the configuration loads")
    }
}

pub fn git(directory: &Path, arguments: &[&str]) {
    let status = Command::new("git")
        .args(arguments)
        .current_dir(directory)
        .env("GIT_AUTHOR_NAME", AUTHOR)
        .env("GIT_AUTHOR_EMAIL", EMAIL)
        .env("GIT_COMMITTER_NAME", AUTHOR)
        .env("GIT_COMMITTER_EMAIL", EMAIL)
        .status()
        .expect("git runs");
    assert!(status.success(), "git {arguments:?} failed");
}

pub fn build_scenario(script: Value, repair_budget: u32, candidates: u8) -> Scenario {
    let home = TempDir::new().expect("a temporary home");
    let repository_root = TempDir::new().expect("a temporary repository");
    let repository = repository_root.path().to_path_buf();

    copy_fixture(
        &workspace_root().join("fixtures").join("repairable-python"),
        &repository,
    );

    let script_path = repository.join(".heikas").join("agent-script.json");
    std::fs::write(
        &script_path,
        serde_json::to_vec_pretty(&script).expect("the script encodes"),
    )
    .expect("the script writes");

    let configuration_path = repository.join(".heikas").join("forge.toml");
    let configuration =
        std::fs::read_to_string(&configuration_path).expect("the configuration reads");
    let updated = configuration
        .replace(
            "model = \"heikas-deterministic-fixture-1.0\"",
            &format!(
                "model = \"heikas-deterministic-fixture-1.0\"\nfixture_script = \"{}\"",
                script_path.display()
            ),
        )
        .replace(
            "max_repairs_per_candidate = 2",
            &format!("max_repairs_per_candidate = {repair_budget}"),
        )
        .replace("candidates = 3", &format!("candidates = {candidates}"))
        .replace(
            "max_parallel_candidates = 3",
            &format!("max_parallel_candidates = {candidates}"),
        );
    std::fs::write(&configuration_path, updated).expect("the configuration writes");

    git(&repository, &["init", "--quiet"]);
    git(&repository, &["symbolic-ref", "HEAD", "refs/heads/main"]);
    git(&repository, &["config", "user.name", AUTHOR]);
    git(&repository, &["config", "user.email", EMAIL]);
    git(&repository, &["config", "commit.gpgsign", "false"]);
    git(&repository, &["add", "-A"]);
    git(
        &repository,
        &["commit", "--quiet", "--message", "Add the invoice module"],
    );

    let layout = StoreLayout::new(home.path().to_path_buf());
    let runtime = build_runtime(layout).expect("the runtime builds");

    Scenario {
        home,
        repository_root,
        repository,
        script: script_path,
        runtime,
    }
}

fn copy_fixture(source: &Path, destination: &Path) {
    for entry in walkdir::WalkDir::new(source)
        .into_iter()
        .filter_map(Result::ok)
    {
        let relative = match entry.path().strip_prefix(source) {
            Ok(relative) => relative,
            Err(_) => continue,
        };
        if relative.as_os_str().is_empty() {
            continue;
        }
        let target = destination.join(relative);
        if entry.file_type().is_dir() {
            std::fs::create_dir_all(&target).expect("the directory creates");
        } else if entry.file_type().is_file() {
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent).expect("the directory creates");
            }
            std::fs::copy(entry.path(), &target).expect("the file copies");
        }
    }
}

pub fn correct_invoice() -> String {
    r#"from decimal import Decimal, ROUND_HALF_UP


LINE_ITEM_PRECISION = Decimal("0.01")


def round_currency(amount):
    return Decimal(amount).quantize(LINE_ITEM_PRECISION, rounding=ROUND_HALF_UP)


def line_total(unit_price, quantity):
    return round_currency(Decimal(str(unit_price)) * Decimal(quantity))


def invoice_total(line_items):
    total = Decimal("0")
    for unit_price, quantity in line_items:
        total += line_total(unit_price, quantity)
    return round_currency(total)
"#
    .to_string()
}

pub fn wrong_invoice() -> String {
    correct_invoice().replace("ROUND_HALF_UP", "ROUND_HALF_DOWN")
}

pub fn weakened_tests() -> String {
    r#"import sys
from decimal import Decimal
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))

from invoice import invoice_total


def test_invoice_total_sums_rounded_line_totals():
    assert invoice_total([("1.005", 1), ("2.005", 1)]) == Decimal("3.02")
"#
    .to_string()
}

pub fn plan_document() -> String {
    let headings = heikas_domain::plan::REQUIRED_PLAN_HEADINGS;
    let mut document = String::from("# Implementation plan\n\n");
    for heading in headings {
        document.push_str(&format!("## {heading}\n\n"));
        if heading == "Files expected to change" {
            document.push_str("- `src/invoice.py`\n\n");
        } else {
            document.push_str("Replace banker's rounding with rounding half away from zero.\n\n");
        }
    }
    document
}

pub fn planner_step() -> Value {
    json!({
        "role": "planner",
        "structured_response": {
            "plan_markdown": plan_document(),
            "expected_files": ["src/invoice.py"],
            "summary": "Round half away from zero."
        }
    })
}

pub fn implementer_step(ordinal: u8, contents: &str) -> Value {
    json!({
        "role": "implementer",
        "candidate_ordinal": ordinal,
        "attempt": 1,
        "writes": [{ "path": "src/invoice.py", "contents": contents }],
        "structured_response": {
            "summary": "Adjusted the rounding policy.",
            "changed_files": ["src/invoice.py"],
            "tests_added": []
        }
    })
}

pub fn implementer_writing(ordinal: u8, writes: Vec<(&str, String)>) -> Value {
    json!({
        "role": "implementer",
        "candidate_ordinal": ordinal,
        "attempt": 1,
        "writes": writes
            .into_iter()
            .map(|(path, contents)| json!({ "path": path, "contents": contents }))
            .collect::<Vec<_>>(),
        "structured_response": {
            "summary": "Applied the change.",
            "changed_files": ["src/invoice.py"],
            "tests_added": []
        }
    })
}

pub fn repairer_step(ordinal: u8, attempt: u32, writes: Vec<(&str, String)>) -> Value {
    json!({
        "role": "repairer",
        "candidate_ordinal": ordinal,
        "attempt": attempt,
        "writes": writes
            .into_iter()
            .map(|(path, contents)| json!({ "path": path, "contents": contents }))
            .collect::<Vec<_>>(),
        "structured_response": {
            "summary": "Repaired the candidate.",
            "changed_files": ["src/invoice.py"],
            "tests_added": []
        }
    })
}

pub fn script(steps: Vec<Value>) -> Value {
    json!({
        "schema_version": 1,
        "model_identity": "heikas-integration-fixture",
        "steps": steps
    })
}

pub async fn approve_plan(scenario: &Scenario, run: RunId) {
    scenario
        .service()
        .approve_plan(
            run,
            None,
            Some("approved by the integration test".to_string()),
        )
        .await
        .expect("the plan approves");
}
