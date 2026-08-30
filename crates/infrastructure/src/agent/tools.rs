use std::path::{Path, PathBuf};
use std::sync::Arc;

use heikas_application::error::{ApplicationError, ApplicationResult};
use heikas_application::ports::agent::ToolPolicy;
use heikas_application::ports::process::{CancellationSignal, ProcessRequest, ProcessRunner};
use heikas_domain::command::{CommandId, CommandSpecification};
use heikas_domain::path_policy::PathAccess;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::str::FromStr;
use walkdir::WalkDir;

use crate::paths::{canonical_root, confine, relative_within};

pub const COMPLETION_TOOL: &str = "heikas_complete";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: Value,
}

impl ToolDefinition {
    pub fn to_openai_tool(&self) -> Value {
        json!({
            "type": "function",
            "function": {
                "name": self.name,
                "description": self.description,
                "parameters": self.parameters,
            }
        })
    }
}

pub struct ToolExecutor {
    worktree: PathBuf,
    policy: ToolPolicy,
    commands: Vec<CommandSpecification>,
    processes: Arc<dyn ProcessRunner>,
    cancellation: CancellationSignal,
    output_budget_bytes: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ToolExecution {
    pub result: Value,
    pub accepted: bool,
    pub rejection_reason: Option<String>,
    pub summary: String,
    pub completion: Option<Value>,
}

impl ToolExecutor {
    pub fn new(
        worktree: PathBuf,
        policy: ToolPolicy,
        commands: Vec<CommandSpecification>,
        processes: Arc<dyn ProcessRunner>,
        cancellation: CancellationSignal,
        output_budget_bytes: u64,
    ) -> Self {
        Self {
            worktree,
            policy,
            commands,
            processes,
            cancellation,
            output_budget_bytes,
        }
    }

    pub fn definitions(&self, completion_schema: &Value) -> Vec<ToolDefinition> {
        let mut definitions = Vec::new();
        if self.policy.allow_read {
            definitions.push(ToolDefinition {
                name: "list_entries".to_string(),
                description: "List directory entries inside the assigned worktree.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "A worktree relative directory path. Use `.` for the root." },
                        "max_entries": { "type": "integer", "minimum": 1, "maximum": 2000 }
                    },
                    "required": ["path"],
                    "additionalProperties": false
                }),
            });
            definitions.push(ToolDefinition {
                name: "read_file".to_string(),
                description: "Read a text file inside the assigned worktree.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "path": { "type": "string" },
                        "start_line": { "type": "integer", "minimum": 1 },
                        "line_count": { "type": "integer", "minimum": 1, "maximum": 4000 }
                    },
                    "required": ["path"],
                    "additionalProperties": false
                }),
            });
        }
        if self.policy.allow_search {
            definitions.push(ToolDefinition {
                name: "search_text".to_string(),
                description: "Search the worktree for a literal string.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "query": { "type": "string" },
                        "extension": { "type": "string" },
                        "max_results": { "type": "integer", "minimum": 1, "maximum": 200 }
                    },
                    "required": ["query"],
                    "additionalProperties": false
                }),
            });
        }
        if self.policy.allow_git_inspection {
            definitions.push(ToolDefinition {
                name: "inspect_git".to_string(),
                description: "Inspect the Git status or diff of the assigned worktree.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "mode": { "type": "string", "enum": ["status", "diff"] }
                    },
                    "required": ["mode"],
                    "additionalProperties": false
                }),
            });
        }
        if self.policy.allow_write {
            definitions.push(ToolDefinition {
                name: "write_file".to_string(),
                description: "Write a complete text file inside the assigned worktree.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "path": { "type": "string" },
                        "contents": { "type": "string" }
                    },
                    "required": ["path", "contents"],
                    "additionalProperties": false
                }),
            });
        }
        if self.policy.allow_patch {
            definitions.push(ToolDefinition {
                name: "apply_patch".to_string(),
                description: "Apply a unified diff to the assigned worktree.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "patch": { "type": "string" }
                    },
                    "required": ["patch"],
                    "additionalProperties": false
                }),
            });
        }
        if self.policy.allow_delete {
            definitions.push(ToolDefinition {
                name: "delete_file".to_string(),
                description: "Delete a file inside the assigned worktree.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "path": { "type": "string" }
                    },
                    "required": ["path"],
                    "additionalProperties": false
                }),
            });
        }
        if !self.policy.allowed_command_ids.is_empty() {
            let identifiers: Vec<String> = self
                .policy
                .allowed_command_ids
                .iter()
                .map(|id| id.to_string())
                .collect();
            definitions.push(ToolDefinition {
                name: "run_named_command".to_string(),
                description: "Run one of the configured named commands. Arbitrary shell commands are not available.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "command_id": { "type": "string", "enum": identifiers }
                    },
                    "required": ["command_id"],
                    "additionalProperties": false
                }),
            });
        }
        definitions.push(ToolDefinition {
            name: COMPLETION_TOOL.to_string(),
            description: "Return the final structured result and finish the task.".to_string(),
            parameters: completion_schema.clone(),
        });
        definitions
    }

    pub async fn execute(&self, name: &str, arguments: &Value) -> ApplicationResult<ToolExecution> {
        match name {
            "list_entries" => self.list_entries(arguments),
            "read_file" => self.read_file(arguments),
            "search_text" => self.search_text(arguments),
            "inspect_git" => self.inspect_git(arguments).await,
            "write_file" => self.write_file(arguments),
            "apply_patch" => self.apply_patch(arguments).await,
            "delete_file" => self.delete_file(arguments),
            "run_named_command" => self.run_named_command(arguments).await,
            COMPLETION_TOOL => Ok(ToolExecution {
                result: json!({ "accepted": true }),
                accepted: true,
                rejection_reason: None,
                summary: "structured completion returned".to_string(),
                completion: Some(arguments.clone()),
            }),
            other => Ok(rejected(format!("the tool `{other}` is not available"))),
        }
    }

    fn require(&self, allowed: bool, tool: &str) -> Option<ToolExecution> {
        if allowed {
            None
        } else {
            Some(rejected(format!(
                "the tool `{tool}` is not permitted for this node"
            )))
        }
    }

    fn list_entries(&self, arguments: &Value) -> ApplicationResult<ToolExecution> {
        if let Some(rejection) = self.require(self.policy.allow_read, "list_entries") {
            return Ok(rejection);
        }
        let raw = arguments
            .get("path")
            .and_then(Value::as_str)
            .unwrap_or(".")
            .to_string();
        let max_entries = arguments
            .get("max_entries")
            .and_then(Value::as_u64)
            .unwrap_or(400)
            .min(2_000) as usize;
        let root = canonical_root(&self.worktree)?;
        let directory = if raw == "." || raw.is_empty() {
            root.clone()
        } else {
            match confine(
                &self.worktree,
                &raw,
                PathAccess::Read,
                &self.policy.path_policy,
            ) {
                Ok(path) => path.absolute,
                Err(error) => return Ok(rejected(error.to_string())),
            }
        };
        if !directory.is_dir() {
            return Ok(rejected(format!("`{raw}` is not a directory")));
        }
        let mut entries = Vec::new();
        for entry in WalkDir::new(&directory)
            .max_depth(1)
            .min_depth(1)
            .sort_by_file_name()
            .into_iter()
            .filter_map(Result::ok)
        {
            if entry.file_name().to_string_lossy() == ".git" {
                continue;
            }
            let Some(relative) = relative_within(&root, entry.path()) else {
                continue;
            };
            entries.push(json!({
                "path": relative,
                "kind": if entry.file_type().is_dir() { "directory" } else { "file" },
                "bytes": entry.metadata().map(|data| data.len()).unwrap_or(0),
            }));
            if entries.len() >= max_entries {
                break;
            }
        }
        let count = entries.len();
        Ok(accepted(
            json!({ "entries": entries }),
            format!("listed {count} entries under `{raw}`"),
        ))
    }

    fn read_file(&self, arguments: &Value) -> ApplicationResult<ToolExecution> {
        if let Some(rejection) = self.require(self.policy.allow_read, "read_file") {
            return Ok(rejection);
        }
        let Some(raw) = arguments.get("path").and_then(Value::as_str) else {
            return Ok(rejected("the `path` argument is required".to_string()));
        };
        let confined = match confine(
            &self.worktree,
            raw,
            PathAccess::Read,
            &self.policy.path_policy,
        ) {
            Ok(path) => path,
            Err(error) => return Ok(rejected(error.to_string())),
        };
        let metadata = match std::fs::metadata(&confined.absolute) {
            Ok(metadata) => metadata,
            Err(error) => return Ok(rejected(format!("`{raw}` could not be read: {error}"))),
        };
        if metadata.len() > self.policy.path_policy.maximum_read_bytes {
            return Ok(rejected(format!(
                "`{raw}` is {} bytes which exceeds the {} byte read limit",
                metadata.len(),
                self.policy.path_policy.maximum_read_bytes
            )));
        }
        let bytes = match std::fs::read(&confined.absolute) {
            Ok(bytes) => bytes,
            Err(error) => return Ok(rejected(format!("`{raw}` could not be read: {error}"))),
        };
        let Ok(text) = String::from_utf8(bytes) else {
            return Ok(rejected(format!("`{raw}` is not a UTF-8 text file")));
        };
        let start_line = arguments
            .get("start_line")
            .and_then(Value::as_u64)
            .unwrap_or(1)
            .max(1) as usize;
        let line_count = arguments
            .get("line_count")
            .and_then(Value::as_u64)
            .unwrap_or(4_000)
            .min(4_000) as usize;
        let lines: Vec<&str> = text.lines().collect();
        let total = lines.len();
        let selected: String = lines
            .iter()
            .skip(start_line - 1)
            .take(line_count)
            .copied()
            .collect::<Vec<_>>()
            .join("\n");
        Ok(accepted(
            json!({
                "path": confined.relative.as_str(),
                "total_lines": total,
                "start_line": start_line,
                "contents": selected,
            }),
            format!("read `{}`", confined.relative),
        ))
    }

    fn search_text(&self, arguments: &Value) -> ApplicationResult<ToolExecution> {
        if let Some(rejection) = self.require(self.policy.allow_search, "search_text") {
            return Ok(rejection);
        }
        let Some(query) = arguments.get("query").and_then(Value::as_str) else {
            return Ok(rejected("the `query` argument is required".to_string()));
        };
        if query.is_empty() {
            return Ok(rejected(
                "the `query` argument must not be empty".to_string(),
            ));
        }
        let extension = arguments.get("extension").and_then(Value::as_str);
        let max_results = arguments
            .get("max_results")
            .and_then(Value::as_u64)
            .unwrap_or(60)
            .min(200) as usize;
        let root = canonical_root(&self.worktree)?;
        let mut matches = Vec::new();
        for entry in WalkDir::new(&root)
            .into_iter()
            .filter_entry(|entry| entry.file_name().to_string_lossy() != ".git")
            .filter_map(Result::ok)
            .filter(|entry| entry.file_type().is_file())
        {
            if let Some(extension) = extension {
                if entry.path().extension().and_then(|value| value.to_str()) != Some(extension) {
                    continue;
                }
            }
            let Ok(metadata) = entry.metadata() else {
                continue;
            };
            if metadata.len() > self.policy.path_policy.maximum_read_bytes {
                continue;
            }
            let Ok(contents) = std::fs::read_to_string(entry.path()) else {
                continue;
            };
            let Some(relative) = relative_within(&root, entry.path()) else {
                continue;
            };
            for (index, line) in contents.lines().enumerate() {
                if line.contains(query) {
                    matches.push(json!({
                        "path": relative,
                        "line": index + 1,
                        "text": line.chars().take(400).collect::<String>(),
                    }));
                    if matches.len() >= max_results {
                        break;
                    }
                }
            }
            if matches.len() >= max_results {
                break;
            }
        }
        let count = matches.len();
        Ok(accepted(
            json!({ "matches": matches }),
            format!("found {count} matches for the search"),
        ))
    }

    async fn inspect_git(&self, arguments: &Value) -> ApplicationResult<ToolExecution> {
        if let Some(rejection) = self.require(self.policy.allow_git_inspection, "inspect_git") {
            return Ok(rejection);
        }
        let mode = arguments
            .get("mode")
            .and_then(Value::as_str)
            .unwrap_or("status");
        let args = match mode {
            "status" => vec![
                "--no-pager".to_string(),
                "status".to_string(),
                "--porcelain=v1".to_string(),
                "--untracked-files=all".to_string(),
            ],
            "diff" => vec![
                "--no-pager".to_string(),
                "diff".to_string(),
                "--stat".to_string(),
                "HEAD".to_string(),
            ],
            other => return Ok(rejected(format!("the mode `{other}` is not supported"))),
        };
        let request = ProcessRequest {
            program: "git".to_string(),
            args,
            working_directory: self.worktree.clone(),
            environment: Vec::new(),
            stdin: None,
            timeout_seconds: 60,
            max_output_bytes: 262_144,
            label: format!("inspect_git:{mode}"),
        };
        let outcome = self
            .processes
            .run(request, self.cancellation.clone())
            .await?;
        Ok(accepted(
            json!({
                "mode": mode,
                "stdout": outcome.stdout_text(),
                "exit_code": outcome.exit_code,
            }),
            format!("inspected git {mode}"),
        ))
    }

    fn write_file(&self, arguments: &Value) -> ApplicationResult<ToolExecution> {
        if let Some(rejection) = self.require(self.policy.allow_write, "write_file") {
            return Ok(rejection);
        }
        let Some(raw) = arguments.get("path").and_then(Value::as_str) else {
            return Ok(rejected("the `path` argument is required".to_string()));
        };
        let Some(contents) = arguments.get("contents").and_then(Value::as_str) else {
            return Ok(rejected("the `contents` argument is required".to_string()));
        };
        if contents.len() as u64 > self.policy.path_policy.maximum_write_bytes {
            return Ok(rejected(format!(
                "the content is {} bytes which exceeds the {} byte write limit",
                contents.len(),
                self.policy.path_policy.maximum_write_bytes
            )));
        }
        let confined = match confine(
            &self.worktree,
            raw,
            PathAccess::Write,
            &self.policy.path_policy,
        ) {
            Ok(path) => path,
            Err(error) => return Ok(rejected(error.to_string())),
        };
        if let Some(parent) = confined.absolute.parent() {
            crate::atomic::ensure_directory(parent)?;
        }
        crate::atomic::write_atomic(&confined.absolute, contents.as_bytes())?;
        Ok(accepted(
            json!({ "path": confined.relative.as_str(), "bytes": contents.len() }),
            format!("wrote `{}`", confined.relative),
        ))
    }

    fn delete_file(&self, arguments: &Value) -> ApplicationResult<ToolExecution> {
        if let Some(rejection) = self.require(self.policy.allow_delete, "delete_file") {
            return Ok(rejection);
        }
        let Some(raw) = arguments.get("path").and_then(Value::as_str) else {
            return Ok(rejected("the `path` argument is required".to_string()));
        };
        let confined = match confine(
            &self.worktree,
            raw,
            PathAccess::Delete,
            &self.policy.path_policy,
        ) {
            Ok(path) => path,
            Err(error) => return Ok(rejected(error.to_string())),
        };
        if !confined.absolute.is_file() {
            return Ok(rejected(format!("`{raw}` is not a file")));
        }
        std::fs::remove_file(&confined.absolute)
            .map_err(|error| crate::atomic::storage(&confined.absolute, "remove", error))?;
        Ok(accepted(
            json!({ "path": confined.relative.as_str() }),
            format!("deleted `{}`", confined.relative),
        ))
    }

    async fn enumerate_patch(
        &self,
        patch_file: &Path,
        reverse: bool,
    ) -> ApplicationResult<Result<Vec<String>, String>> {
        let mut args = vec![
            "--no-pager".to_string(),
            "apply".to_string(),
            "--numstat".to_string(),
            "-z".to_string(),
        ];
        if reverse {
            args.push("--reverse".to_string());
        }
        args.push(patch_file.display().to_string());
        let request = ProcessRequest {
            program: "git".to_string(),
            args,
            working_directory: self.worktree.clone(),
            environment: Vec::new(),
            stdin: None,
            timeout_seconds: 60,
            max_output_bytes: 262_144,
            label: "apply_patch:enumerate".to_string(),
        };
        let outcome = self
            .processes
            .run(request, self.cancellation.clone())
            .await?;
        if !outcome.succeeded() {
            return Ok(Err(format!(
                "the patch could not be read: {}",
                outcome.stderr_text().trim()
            )));
        }
        Ok(Ok(parse_numstat_paths(&outcome.stdout)))
    }

    async fn affected_patch_paths(
        &self,
        patch_file: &Path,
    ) -> ApplicationResult<Result<Vec<String>, String>> {
        let forward = match self.enumerate_patch(patch_file, false).await? {
            Ok(paths) => paths,
            Err(reason) => return Ok(Err(reason)),
        };
        let reverse: Vec<String> = self
            .enumerate_patch(patch_file, true)
            .await?
            .unwrap_or_default();
        let mut affected = forward;
        affected.extend(reverse);
        affected.sort();
        affected.dedup();
        Ok(Ok(affected))
    }

    async fn apply_patch(&self, arguments: &Value) -> ApplicationResult<ToolExecution> {
        if let Some(rejection) = self.require(self.policy.allow_patch, "apply_patch") {
            return Ok(rejection);
        }
        let Some(patch) = arguments.get("patch").and_then(Value::as_str) else {
            return Ok(rejected("the `patch` argument is required".to_string()));
        };
        if let Some(mode) = declares_symbolic_link(patch) {
            return Ok(rejected(format!(
                "a patch may not create or alter a symbolic link (file mode {mode})"
            )));
        }
        let directory = tempfile::Builder::new()
            .prefix("heikas-agent-patch-")
            .tempdir()
            .map_err(|error| ApplicationError::Storage(error.to_string()))?;
        let path = directory.path().join("change.patch");
        std::fs::write(&path, patch)
            .map_err(|error| crate::atomic::storage(&path, "write", error))?;
        let affected = match self.affected_patch_paths(&path).await? {
            Ok(paths) => paths,
            Err(reason) => return Ok(rejected(reason)),
        };
        if affected.is_empty() {
            return Ok(rejected(
                "the patch does not describe any file change".to_string(),
            ));
        }
        for candidate in &affected {
            if let Err(error) = confine(
                &self.worktree,
                candidate,
                PathAccess::Write,
                &self.policy.path_policy,
            ) {
                return Ok(rejected(error.to_string()));
            }
        }
        let request = ProcessRequest {
            program: "git".to_string(),
            args: vec![
                "--no-pager".to_string(),
                "apply".to_string(),
                "--whitespace=nowarn".to_string(),
                path.display().to_string(),
            ],
            working_directory: self.worktree.clone(),
            environment: Vec::new(),
            stdin: None,
            timeout_seconds: 120,
            max_output_bytes: 262_144,
            label: "apply_patch".to_string(),
        };
        let outcome = self
            .processes
            .run(request, self.cancellation.clone())
            .await?;
        if outcome.succeeded() {
            Ok(accepted(
                json!({ "applied": true }),
                "applied the supplied patch".to_string(),
            ))
        } else {
            Ok(rejected(format!(
                "the patch did not apply: {}",
                outcome.stderr_text().trim()
            )))
        }
    }

    async fn run_named_command(&self, arguments: &Value) -> ApplicationResult<ToolExecution> {
        let Some(raw) = arguments.get("command_id").and_then(Value::as_str) else {
            return Ok(rejected(
                "the `command_id` argument is required".to_string(),
            ));
        };
        let Ok(command_id) = CommandId::from_str(raw) else {
            return Ok(rejected(format!(
                "`{raw}` is not a valid command identifier"
            )));
        };
        if !self.policy.allowed_command_ids.contains(&command_id) {
            return Ok(rejected(format!(
                "the command `{command_id}` is not permitted for this node"
            )));
        }
        let Some(specification) = self
            .commands
            .iter()
            .find(|command| command.id == command_id)
        else {
            return Ok(rejected(format!(
                "the command `{command_id}` is not configured"
            )));
        };
        let request = crate::process::request_for_command(
            specification,
            &self.worktree,
            self.output_budget_bytes.min(1_048_576),
        )?;
        let outcome = self
            .processes
            .run(request, self.cancellation.clone())
            .await?;
        Ok(accepted(
            json!({
                "command_id": command_id.as_str(),
                "exit_code": outcome.exit_code,
                "timed_out": outcome.timed_out,
                "stdout": tail(&outcome.stdout_text(), 8_000),
                "stderr": tail(&outcome.stderr_text(), 8_000),
            }),
            format!("ran the configured command `{command_id}`"),
        ))
    }

    pub fn worktree(&self) -> &Path {
        &self.worktree
    }
}

fn accepted(result: Value, summary: String) -> ToolExecution {
    ToolExecution {
        result,
        accepted: true,
        rejection_reason: None,
        summary,
        completion: None,
    }
}

fn rejected(reason: String) -> ToolExecution {
    ToolExecution {
        result: json!({ "error": reason }),
        accepted: false,
        rejection_reason: Some(reason.clone()),
        summary: reason,
        completion: None,
    }
}

fn tail(value: &str, limit: usize) -> String {
    if value.len() <= limit {
        return value.to_string();
    }
    let start = value.len() - limit;
    format!("[output truncated]\n{}", &value[start..])
}

fn parse_numstat_paths(output: &[u8]) -> Vec<String> {
    let mut paths = Vec::new();
    for record in output.split(|byte| *byte == 0) {
        if record.is_empty() {
            continue;
        }
        let text = String::from_utf8_lossy(record);
        let mut fields = text.splitn(3, '\t');
        let (Some(_added), Some(_deleted), Some(path)) =
            (fields.next(), fields.next(), fields.next())
        else {
            continue;
        };
        let trimmed = path.trim_matches(|character| character == '\n' || character == '\r');
        if !trimmed.is_empty() {
            paths.push(trimmed.to_string());
        }
    }
    paths
}

fn declares_symbolic_link(patch: &str) -> Option<&str> {
    for line in patch.lines() {
        let trimmed = line.trim_end();
        for prefix in [
            "new file mode ",
            "new mode ",
            "old mode ",
            "deleted file mode ",
        ] {
            if let Some(mode) = trimmed.strip_prefix(prefix) {
                if mode.trim() == "120000" {
                    return Some("120000");
                }
            }
        }
    }
    None
}
