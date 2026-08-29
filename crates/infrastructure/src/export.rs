use std::io::{Cursor, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use heikas_application::error::{ApplicationError, ApplicationResult};
use heikas_application::ports::observability::Redactor;
use heikas_application::ports::runtime::{EvidenceExporter, ExportOutcome};
use heikas_domain::identity::RunId;
use walkdir::WalkDir;

use crate::atomic::{ensure_directory, storage, write_atomic};
use crate::layout::StoreLayout;

const TEXT_EXTENSIONS: [&str; 12] = [
    "json", "jsonl", "md", "log", "txt", "patch", "diff", "xml", "toml", "yaml", "yml", "info",
];

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
        {
            let mut writer = zip::ZipWriter::new(&mut buffer);
            let options: zip::write::FileOptions<'_, ()> = zip::write::FileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated);

            let mut roots = vec![(run_directory.clone(), format!("run-{}", run_id))];
            if include_worktrees {
                let worktrees = self.layout.run_worktrees(run_id);
                if worktrees.exists() {
                    roots.push((worktrees, format!("worktrees-{}", run_id)));
                }
            }

            for (root, prefix) in roots {
                for entry in WalkDir::new(&root)
                    .into_iter()
                    .filter_entry(|entry| entry.file_name() != "dispatcher.lock")
                    .filter_map(Result::ok)
                    .filter(|entry| entry.file_type().is_file())
                {
                    let Ok(relative) = entry.path().strip_prefix(&root) else {
                        continue;
                    };
                    let name = format!("{prefix}/{}", relative.to_string_lossy().replace('\\', "/"));
                    let contents = std::fs::read(entry.path())
                        .map_err(|error| storage(entry.path(), "read", error))?;
                    let redacted = if is_text(entry.path()) {
                        self.redactor.redact_bytes(&contents)
                    } else {
                        contents
                    };
                    writer
                        .start_file(name, options)
                        .map_err(|error| ApplicationError::Storage(error.to_string()))?;
                    writer
                        .write_all(&redacted)
                        .map_err(|error| ApplicationError::Storage(error.to_string()))?;
                    entry_count += 1;
                }
            }

            let manifest = serde_json::json!({
                "schema_version": 1,
                "run_id": run_id,
                "entries": entry_count,
                "includes_worktrees": include_worktrees,
                "redacted": true,
            });
            writer
                .start_file("export-manifest.json", options)
                .map_err(|error| ApplicationError::Storage(error.to_string()))?;
            writer
                .write_all(&serde_json::to_vec_pretty(&manifest)?)
                .map_err(|error| ApplicationError::Storage(error.to_string()))?;
            entry_count += 1;
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
            redacted: true,
        })
    }
}

fn is_text(path: &Path) -> bool {
    path.extension()
        .and_then(|value| value.to_str())
        .map(|extension| TEXT_EXTENSIONS.contains(&extension))
        .unwrap_or(false)
}
