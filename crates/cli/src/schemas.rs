use std::path::Path;

use heikas_application::configuration::EffectiveConfiguration;
use heikas_application::model::detail::RunDetail;
use heikas_application::model::doctor::DoctorReport;
use heikas_application::model::request::CreateRunRequest;
use heikas_application::model::run_summary::{CandidateView, RunSummary, TimelineEntry};
use heikas_domain::event::DurableEvent;
use heikas_domain::node::NodeResult;
use heikas_domain::review::ReviewReport;
use heikas_domain::score::Ranking;
use heikas_domain::state::RunProjection;
use heikas_application::error::{ApplicationError, ApplicationResult};
use schemars::schema_for;

pub fn write_all(output: &Path) -> ApplicationResult<Vec<String>> {
    std::fs::create_dir_all(output).map_err(|error| {
        ApplicationError::Storage(format!("could not create `{}`: {error}", output.display()))
    })?;
    let documents = vec![
        ("run.schema.json", serde_json::to_value(schema_for!(RunProjection))?),
        ("event.schema.json", serde_json::to_value(schema_for!(DurableEvent))?),
        (
            "node-result.schema.json",
            serde_json::to_value(schema_for!(NodeResult))?,
        ),
        (
            "review-report.schema.json",
            serde_json::to_value(schema_for!(ReviewReport))?,
        ),
        (
            "ranking.schema.json",
            serde_json::to_value(schema_for!(Ranking))?,
        ),
        (
            "configuration.schema.json",
            serde_json::to_value(schema_for!(EffectiveConfiguration))?,
        ),
        (
            "run-summary.schema.json",
            serde_json::to_value(schema_for!(RunSummary))?,
        ),
        (
            "run-detail.schema.json",
            serde_json::to_value(schema_for!(RunDetail))?,
        ),
        (
            "candidate-view.schema.json",
            serde_json::to_value(schema_for!(CandidateView))?,
        ),
        (
            "timeline-entry.schema.json",
            serde_json::to_value(schema_for!(TimelineEntry))?,
        ),
        (
            "doctor-report.schema.json",
            serde_json::to_value(schema_for!(DoctorReport))?,
        ),
        (
            "create-run-request.schema.json",
            serde_json::to_value(schema_for!(CreateRunRequest))?,
        ),
    ];
    let mut written = Vec::new();
    for (name, document) in documents {
        let path = output.join(name);
        let mut bytes = serde_json::to_vec_pretty(&document)?;
        bytes.push(b'\n');
        std::fs::write(&path, bytes).map_err(|error| {
            ApplicationError::Storage(format!("could not write `{}`: {error}", path.display()))
        })?;
        written.push(path.display().to_string());
    }
    Ok(written)
}
