use std::io::{Cursor, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use heikas_application::error::{ApplicationError, ApplicationResult};
use heikas_application::ports::observability::Redactor;
use heikas_application::ports::runtime::{EvidenceExporter, ExportOutcome};
use heikas_domain::identity::RunId;
use heikas_domain::path_policy::{default_sensitive_patterns, RelativeWorkspacePath};
use walkdir::WalkDir;

use crate::atomic::{ensure_directory, storage, write_atomic};
use crate::layout::StoreLayout;
use crate::paths::GlobPatternMatcher;
use heikas_domain::path_policy::PatternMatcher;

pub struct ZipEvidenceExporter {
    layout: StoreLayout,
    redactor: Arc<dyn Redactor>,
}

impl ZipEvidenceExporter {
    pub fn new(layout: StoreLayout, redactor: Arc<dyn Redactor>) -> Self {
        Self { layout, redactor }
    }

    fn resolve_output(&self, run_id: RunId, output_path: &Path) -> PathBuf {
        if output_path.extension().is_some() {
            output_path.to_path_buf()
        } else {
            output_path.join(format!("heikas-run-{}.zip", run_id.short()))
        }
    }
}

#[async_trait]
impl EvidenceExporter for ZipEvidenceExporter {
    async fn export(
        &self,
        run_id: RunId,
        output_path: &Path,
        include_worktrees: bool,
    ) -> ApplicationResult<ExportOutcome> {
        let run_directory = self.layout.run_directory(run_id);
        if !run_directory.exists() {
            return Err(ApplicationError::RunNotFound(run_id));
        }
        let destination = self.resolve_output(run_id, output_path);
        if let Some(parent) = destination.parent() {
            ensure_directory(parent)?;
        }

        let mut buffer = Cursor::new(Vec::new());
        let mut entry_count = 0u64;
        let mut redacted_entries = 0u64;
        let mut unredactable_entries = 0u64;
        let mut excluded_sensitive_paths: Vec<String> = Vec::new();
        {
            let mut writer = zip::ZipWriter::new(&mut buffer);
            let options: zip::write::FileOptions<'_, ()> = zip::write::FileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated);

            let mut roots = vec![(run_directory.clone(), format!("run-{run_id}"), false)];
            if include_worktrees {
                let worktrees = self.layout.run_worktrees(run_id);
                if worktrees.exists() {
                    roots.push((worktrees, format!("worktrees-{run_id}"), true));
                }
            }

            for (root, prefix, screen_sensitive) in roots {
                for entry in WalkDir::new(&root)
                    .into_iter()
                    .filter_entry(|entry| entry.file_name() != "dispatcher.lock")
                    .filter_map(Result::ok)
                    .filter(|entry| entry.file_type().is_file())
                {
                    let Ok(relative) = entry.path().strip_prefix(&root) else {
                        continue;
                    };
                    let relative_text = relative.to_string_lossy().replace('\\', "/");
                    if screen_sensitive && matches_sensitive_pattern(&relative_text) {
                        excluded_sensitive_paths.push(format!("{prefix}/{relative_text}"));
                        continue;
                    }
                    let name = format!("{prefix}/{relative_text}");
                    let contents = std::fs::read(entry.path())
                        .map_err(|error| storage(entry.path(), "read", error))?;
                    let payload = match std::str::from_utf8(&contents) {
                        Ok(text) if !text.contains('\0') => {
                            redacted_entries += 1;
                            self.redactor.redact_text(text).into_bytes()
                        }
                        _ => {
                            unredactable_entries += 1;
                            contents
                        }
                    };
                    writer
                        .start_file(name, options)
                        .map_err(|error| ApplicationError::Storage(error.to_string()))?;
                    writer
                        .write_all(&payload)
                        .map_err(|error| ApplicationError::Storage(error.to_string()))?;
                    entry_count += 1;
                }
            }

            excluded_sensitive_paths.sort();
            let manifest = serde_json::json!({
                "schema_version": 2,
                "run_id": run_id,
                "entries": entry_count,
                "includes_worktrees": include_worktrees,
                "redacted_entries": redacted_entries,
                "unredactable_entries": unredactable_entries,
                "fully_redacted": unredactable_entries == 0,
                "excluded_sensitive_paths": excluded_sensitive_paths,
            });
            writer
                .start_file("export-manifest.json", options)
                .map_err(|error| ApplicationError::Storage(error.to_string()))?;
            writer
                .write_all(&serde_json::to_vec_pretty(&manifest)?)
                .map_err(|error| ApplicationError::Storage(error.to_string()))?;
            entry_count += 1;
            redacted_entries += 1;
            writer
                .finish()
                .map_err(|error| ApplicationError::Storage(error.to_string()))?;
        }

        let bytes = buffer.into_inner();
        write_atomic(&destination, &bytes)?;
        Ok(ExportOutcome {
            archive_path: destination,
            byte_length: bytes.len() as u64,
            entry_count,
            redacted_entries,
            unredactable_entries,
            excluded_sensitive_paths,
        })
    }
}

fn matches_sensitive_pattern(relative: &str) -> bool {
    let Ok(parsed) = RelativeWorkspacePath::parse(relative) else {
        return true;
    };
    default_sensitive_patterns()
        .into_iter()
        .any(|pattern| GlobPatternMatcher.matches(pattern, parsed.as_str()))
}
