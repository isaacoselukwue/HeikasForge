use heikas_application::error::ApplicationResult;
use heikas_application::model::run_summary::TimelineLevel;
use serde::Serialize;

use crate::arguments::TimelineFormat;
use crate::context::CommandContext;
use crate::exit::ExitCode;
use crate::presentation::{Palette, Table};

pub async fn list(
    context: &CommandContext,
    status_filter: Option<String>,
    limit: usize,
) -> ApplicationResult<ExitCode> {
    let mut summaries = context.service().list_runs().await?;
    if let Some(filter) = &status_filter {
        summaries.retain(|summary| summary.status.as_str() == filter);
    }
    summaries.truncate(limit);
    context.emit(&summaries, |palette| {
        if summaries.is_empty() {
            return format!(
                "{}\nNo runs have been created yet. Start one with `heikas run --repo <path> --task <text>`.\n",
                palette.heading("Runs")
            );
        }
        let mut table = Table::new(&[
            "Run", "Status", "Repository", "Task", "Node", "Candidates", "Age", "Winner",
        ]);
        for summary in &summaries {
            table.push(vec![
                summary.run_id.short(),
                palette.run_status(summary.status),
                shorten(&summary.repository_path, 32),
                shorten(&summary.task_title, 40),
                summary
                    .current_nodes
                    .first()
                    .map(|node| node.label().to_string())
                    .unwrap_or_else(|| "-".to_string()),
                format!(
                    "{}/{} eligible",
                    summary.candidate_progress.eligible, summary.candidate_progress.total
                ),
                summary.elapsed.human(),
                summary
                    .winner
                    .as_ref()
                    .map(|winner| winner.to_string())
                    .unwrap_or_else(|| "-".to_string()),
            ]);
        }
        format!("{}\n{}", palette.heading("Runs"), table.render(palette))
    });
    Ok(ExitCode::Success)
}

pub async fn show(context: &CommandContext, reference: &str) -> ApplicationResult<ExitCode> {
    let run_id = context.service().resolve_run_reference(reference).await?;
    let detail = context.service().run_detail(run_id).await?;
    let status = detail.summary.status;
    context.emit(&detail, |palette| render_detail(&detail, palette));
    Ok(ExitCode::for_status(status))
}

fn render_detail(
    detail: &heikas_application::model::detail::RunDetail,
    palette: &Palette,
) -> String {
    let mut text = String::new();
    text.push_str(&palette.heading(&format!("Run {}\n", detail.summary.run_id)));
    text.push_str(&format!("Task: {}\n", detail.summary.task_title));
    text.push_str(&format!("Repository: {}\n", detail.summary.repository_path));
    text.push_str(&format!(
        "Status: {}\n",
        palette.run_status(detail.summary.status)
    ));
    text.push_str(&format!("Elapsed: {}\n", detail.summary.elapsed.human()));
    if detail.summary.demonstration_mode {
        text.push_str(&palette.warning("Demonstration mode is active.\n"));
    }
    if let Some(plan_version) = detail.summary.plan_version {
        text.push_str(&format!(
            "Plan: version {plan_version}, {}\n",
            if detail.summary.plan_approved {
                palette.success("approved")
            } else {
                palette.warning("awaiting approval")
            }
        ));
    }
    if let Some(commit) = &detail.projection.commit {
        text.push_str(&format!(
            "Commit: {} on {} by {}\n",
            commit.commit_hash.short(),
            commit.branch,
            commit.author_name
        ));
    }
    if !detail.candidates.is_empty() {
        text.push_str(&palette.heading("\nCandidates\n"));
        let mut table = Table::new(&[
            "Candidate",
            "Strategy",
            "Status",
            "Rank",
            "Repairs",
            "Changed lines",
            "Coverage",
            "Gate time",
        ]);
        for candidate in &detail.candidates {
            table.push(vec![
                candidate.candidate_id.to_string(),
                candidate.strategy_label.clone(),
                palette.candidate_status(candidate.status),
                candidate
                    .rank
                    .map(|rank| rank.to_string())
                    .unwrap_or_else(|| "-".to_string()),
                format!("{}/{}", candidate.repairs_used, candidate.repair_budget),
                candidate.changed_lines.to_string(),
                candidate
                    .line_coverage_percent
                    .map(|value| format!("{value:.2}%"))
                    .unwrap_or_else(|| "not measured".to_string()),
                candidate.gate_duration.human(),
            ]);
        }
        text.push_str(&table.render(palette));
        for candidate in &detail.candidates {
            if !candidate.exclusion_summaries.is_empty() {
                text.push_str(&format!(
                    "{}: {}\n",
                    candidate.candidate_id,
                    candidate.exclusion_summaries.join("; ")
                ));
            }
        }
    }
    if !detail.ranking_rationale.is_empty() {
        text.push_str(&palette.heading("\nSelection rationale\n"));
        for line in &detail.ranking_rationale {
            text.push_str(&format!("  {line}\n"));
        }
    }
    if let Some(reason) = &detail.summary.recovery_reason {
        text.push_str(&palette.failure(&format!("\nRecovery required: {reason}\n")));
    }
    text
}

pub async fn timeline(
    context: &CommandContext,
    reference: &str,
    format: TimelineFormat,
) -> ApplicationResult<ExitCode> {
    let run_id = context.service().resolve_run_reference(reference).await?;
    let entries = context.service().timeline(run_id).await?;
    match format {
        TimelineFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&entries)?);
        }
        TimelineFormat::Html => {
            println!("{}", render_timeline_html(run_id, &entries));
        }
        TimelineFormat::Text => {
            context.emit(&entries, |palette| {
                let mut table = Table::new(&["Sequence", "Time", "Node", "Candidate", "Event"]);
                for entry in &entries {
                    let summary = match entry.level {
                        TimelineLevel::Failure => palette.failure(&entry.summary),
                        TimelineLevel::Warning => palette.warning(&entry.summary),
                        TimelineLevel::Success => palette.success(&entry.summary),
                        TimelineLevel::Information => entry.summary.clone(),
                    };
                    table.push(vec![
                        entry.sequence.to_string(),
                        entry.recorded_at.to_rfc3339(),
                        entry.node_label.clone().unwrap_or_else(|| "-".to_string()),
                        entry
                            .candidate_id
                            .as_ref()
                            .map(|candidate| candidate.to_string())
                            .unwrap_or_else(|| "-".to_string()),
                        summary,
                    ]);
                }
                format!("{}\n{}", palette.heading("Timeline"), table.render(palette))
            });
        }
    }
    Ok(ExitCode::Success)
}

fn render_timeline_html(
    run_id: heikas_domain::identity::RunId,
    entries: &[heikas_application::model::run_summary::TimelineEntry],
) -> String {
    let mut rows = String::new();
    for entry in entries {
        rows.push_str(&format!(
            "<tr class=\"{}\"><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
            match entry.level {
                TimelineLevel::Failure => "failure",
                TimelineLevel::Warning => "warning",
                TimelineLevel::Success => "success",
                TimelineLevel::Information => "information",
            },
            entry.sequence,
            escape_html(&entry.recorded_at.to_rfc3339()),
            escape_html(entry.node_label.as_deref().unwrap_or("")),
            escape_html(
                &entry
                    .candidate_id
                    .as_ref()
                    .map(|candidate| candidate.to_string())
                    .unwrap_or_default()
            ),
            escape_html(&entry.summary)
        ));
    }
    format!(
        "<!doctype html><html lang=\"en-GB\"><head><meta charset=\"utf-8\"><title>Heikas Forge timeline {run_id}</title><style>body{{font-family:system-ui,sans-serif;background:#0d1117;color:#e6edf3;padding:24px}}table{{border-collapse:collapse;width:100%}}th,td{{border-bottom:1px solid #30363d;padding:6px 10px;text-align:left;font-size:14px}}tr.failure td{{color:#ff7b72}}tr.warning td{{color:#e3b341}}tr.success td{{color:#7ee787}}</style></head><body><h1>Timeline for run {run_id}</h1><table><thead><tr><th>Sequence</th><th>Recorded</th><th>Node</th><th>Candidate</th><th>Event</th></tr></thead><tbody>{rows}</tbody></table></body></html>"
    )
}

fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[derive(Debug, Serialize)]
pub struct LogOutcome {
    pub run_id: String,
    pub records: Vec<heikas_application::ports::observability::StructuredLogRecord>,
    pub total: u64,
}

pub async fn logs(
    context: &CommandContext,
    reference: &str,
    follow: bool,
    limit: usize,
) -> ApplicationResult<ExitCode> {
    let run_id = context.service().resolve_run_reference(reference).await?;
    let reader = context.runtime.log_reader();
    let mut offset = 0u64;
    loop {
        let records = reader.read(run_id, offset, limit).await?;
        let total = reader.count(run_id).await?;
        if !records.is_empty() {
            offset += records.len() as u64;
        }
        let outcome = LogOutcome {
            run_id: run_id.to_string(),
            records: records.clone(),
            total,
        };
        context.emit(&outcome, |palette| {
            let mut text = String::new();
            for record in &records {
                let level = match record.level.as_str() {
                    "ERROR" => palette.failure(&record.level),
                    "WARN" => palette.warning(&record.level),
                    _ => palette.muted(&record.level),
                };
                text.push_str(&format!(
                    "{} {level} {} {}\n",
                    record.recorded_at.to_rfc3339(),
                    record.target,
                    record.message
                ));
            }
            text
        });
        if !follow {
            break;
        }
        let projection = context.service().projection(run_id).await?;
        if projection.status.is_terminal() && offset >= total {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(750)).await;
    }
    Ok(ExitCode::Success)
}

fn shorten(value: &str, limit: usize) -> String {
    if value.chars().count() <= limit {
        return value.to_string();
    }
    let tail: String = value
        .chars()
        .rev()
        .take(limit.saturating_sub(3))
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    format!("...{tail}")
}
