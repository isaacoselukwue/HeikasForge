use std::io::{Cursor, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use heikas_application::error::{ApplicationError, ApplicationResult};
use heikas_application::ports::git::{
    CommitOutcome, CommitRequest, DiffSummary, DirtySnapshot, GitService, RepositoryFacts,
    WorktreeHandle,
};
use heikas_application::ports::process::{ProcessOutcome, ProcessRequest, ProcessRunner};
use heikas_domain::identity::{BranchName, CandidateId, CommitHash, RunId};
use heikas_domain::path_policy::WorktreeRole;
use std::str::FromStr;
use tokio::sync::watch;
use tracing::debug;

use crate::atomic::{ensure_directory, storage};
use crate::layout::StoreLayout;

const GIT_TIMEOUT_SECONDS: u32 = 300;
const GIT_OUTPUT_LIMIT: u64 = 33_554_432;

pub struct CommandLineGitService {
    processes: Arc<dyn ProcessRunner>,
    layout: StoreLayout,
    author_name: String,
}

impl CommandLineGitService {
    pub fn new(
        processes: Arc<dyn ProcessRunner>,
        layout: StoreLayout,
        author_name: String,
    ) -> Self {
        Self {
            processes,
            layout,
            author_name,
        }
    }

    async fn git(
        &self,
        working_directory: &Path,
        args: &[&str],
    ) -> ApplicationResult<ProcessOutcome> {
        self.git_with_environment(working_directory, args, Vec::new())
            .await
    }

    async fn git_with_environment(
        &self,
        working_directory: &Path,
        args: &[&str],
        environment: Vec<(String, String)>,
    ) -> ApplicationResult<ProcessOutcome> {
        let (_sender, receiver) = watch::channel(false);
        let mut arguments = vec!["--no-pager".to_string()];
        arguments.extend(args.iter().map(|value| (*value).to_string()));
        let request = ProcessRequest {
            program: "git".to_string(),
            args: arguments,
            working_directory: working_directory.to_path_buf(),
            environment,
            timeout_seconds: GIT_TIMEOUT_SECONDS,
            max_output_bytes: GIT_OUTPUT_LIMIT,
            label: format!("git {}", args.first().copied().unwrap_or("")),
        };
        let outcome = self.processes.run(request, receiver).await?;
        debug!(args = ?args, exit_code = ?outcome.exit_code, "git command finished");
        Ok(outcome)
    }

    async fn git_checked(
        &self,
        working_directory: &Path,
        args: &[&str],
    ) -> ApplicationResult<String> {
        let outcome = self.git(working_directory, args).await?;
        if !outcome.succeeded() {
            return Err(ApplicationError::Git(format!(
                "`git {}` failed with status {}: {}",
                args.join(" "),
                outcome
                    .exit_code
                    .map(|code| code.to_string())
                    .unwrap_or_else(|| "unknown".to_string()),
                outcome.stderr_text().trim()
            )));
        }
        Ok(outcome.stdout_text())
    }

    async fn config_value(
        &self,
        repository: &Path,
        key: &str,
    ) -> ApplicationResult<Option<String>> {
        let outcome = self.git(repository, &["config", "--get", key]).await?;
        if outcome.succeeded() {
            let value = outcome.stdout_text().trim().to_string();
            Ok(if value.is_empty() { None } else { Some(value) })
        } else {
            Ok(None)
        }
    }

    async fn resolve_default_branch(&self, repository: &Path) -> ApplicationResult<String> {
        let remote = self
            .git(
                repository,
                &["symbolic-ref", "--quiet", "refs/remotes/origin/HEAD"],
            )
            .await?;
        if remote.succeeded() {
            let text = remote.stdout_text();
            if let Some(name) = text.trim().rsplit('/').next() {
                if !name.is_empty() {
                    return Ok(name.to_string());
                }
            }
        }
        let current = self
            .git(repository, &["rev-parse", "--abbrev-ref", "HEAD"])
            .await?;
        if current.succeeded() {
            let name = current.stdout_text().trim().to_string();
            if !name.is_empty() && name != "HEAD" {
                return Ok(name);
            }
        }
        Ok("main".to_string())
    }

    async fn stage_all(&self, worktree: &Path) -> ApplicationResult<()> {
        self.git_checked(worktree, &["add", "--all", "--"]).await?;
        Ok(())
    }

    fn write_temporary_patch(
        &self,
        patch: &[u8],
    ) -> ApplicationResult<(PathBuf, tempfile::TempDir)> {
        let directory = tempfile::Builder::new()
            .prefix("heikas-patch-")
            .tempdir()
            .map_err(|error| ApplicationError::Storage(error.to_string()))?;
        let path = directory.path().join("change.patch");
        let mut file =
            std::fs::File::create(&path).map_err(|error| storage(&path, "create", error))?;
        file.write_all(patch)
            .map_err(|error| storage(&path, "write", error))?;
        file.sync_all()
            .map_err(|error| storage(&path, "synchronise", error))?;
        Ok((path, directory))
    }
}

#[async_trait]
impl GitService for CommandLineGitService {
    async fn inspect(&self, repository: &Path) -> ApplicationResult<RepositoryFacts> {
        if !repository.exists() {
            return Err(ApplicationError::RepositoryUnusable {
                path: repository.display().to_string(),
                detail: "the path does not exist".to_string(),
            });
        }
        let inside = self
            .git(repository, &["rev-parse", "--is-inside-work-tree"])
            .await?;
        if !inside.succeeded() || inside.stdout_text().trim() != "true" {
            return Err(ApplicationError::RepositoryUnusable {
                path: repository.display().to_string(),
                detail: "the path is not inside a Git working tree".to_string(),
            });
        }
        let root = PathBuf::from(
            self.git_checked(repository, &["rev-parse", "--show-toplevel"])
                .await?
                .trim(),
        );
        let git_directory = PathBuf::from(
            self.git_checked(repository, &["rev-parse", "--absolute-git-dir"])
                .await?
                .trim(),
        );
        let head = self.git(repository, &["rev-parse", "HEAD"]).await?;
        if !head.succeeded() {
            return Err(ApplicationError::RepositoryUnusable {
                path: repository.display().to_string(),
                detail: "the repository has no commits yet".to_string(),
            });
        }
        let head_commit = CommitHash::from_str(head.stdout_text().trim())?;
        let default_branch = self.resolve_default_branch(&root).await?;
        let current_branch = self
            .git(&root, &["rev-parse", "--abbrev-ref", "HEAD"])
            .await?
            .stdout_text()
            .trim()
            .to_string();

        let status = self
            .git_checked(
                &root,
                &["status", "--porcelain=v1", "--untracked-files=all"],
            )
            .await?;
        let mut staged = Vec::new();
        let mut unstaged = Vec::new();
        let mut untracked = Vec::new();
        for line in status.lines() {
            if line.len() < 4 {
                continue;
            }
            let indicators = &line[..2];
            let path = line[3..].trim().to_string();
            if indicators == "??" {
                untracked.push(path);
                continue;
            }
            let index_state = indicators.as_bytes()[0] as char;
            let worktree_state = indicators.as_bytes()[1] as char;
            if index_state != ' ' {
                staged.push(path.clone());
            }
            if worktree_state != ' ' {
                unstaged.push(path);
            }
        }

        let signing_enabled = self
            .config_value(&root, "commit.gpgsign")
            .await?
            .map(|value| value.eq_ignore_ascii_case("true"))
            .unwrap_or(false);

        let has_submodules = root.join(".gitmodules").exists();

        Ok(RepositoryFacts {
            root: root.clone(),
            git_directory,
            head_commit,
            default_branch,
            current_branch: if current_branch == "HEAD" {
                None
            } else {
                Some(current_branch)
            },
            is_clean: staged.is_empty() && unstaged.is_empty() && untracked.is_empty(),
            staged_paths: staged,
            unstaged_paths: unstaged,
            untracked_paths: untracked,
            configured_user_name: self.config_value(&root, "user.name").await?,
            configured_user_email: self.config_value(&root, "user.email").await?,
            signing_enabled,
            signing_key: self.config_value(&root, "user.signingkey").await?,
            has_submodules,
        })
    }

    async fn capture_dirty_snapshot(&self, repository: &Path) -> ApplicationResult<DirtySnapshot> {
        let facts = self.inspect(repository).await?;
        let tracked_patch = self
            .git_checked(&facts.root, &["diff", "--binary", "HEAD"])
            .await?
            .into_bytes();
        let mut archive_buffer = Cursor::new(Vec::new());
        {
            let mut writer = zip::ZipWriter::new(&mut archive_buffer);
            let options: zip::write::FileOptions<'_, ()> = zip::write::FileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated);
            for path in &facts.untracked_paths {
                let absolute = facts.root.join(path);
                if !absolute.is_file() {
                    continue;
                }
                let mut contents = Vec::new();
                std::fs::File::open(&absolute)
                    .map_err(|error| storage(&absolute, "open", error))?
                    .read_to_end(&mut contents)
                    .map_err(|error| storage(&absolute, "read", error))?;
                writer
                    .start_file(path.clone(), options)
                    .map_err(|error| ApplicationError::Storage(error.to_string()))?;
                writer
                    .write_all(&contents)
                    .map_err(|error| ApplicationError::Storage(error.to_string()))?;
            }
            writer
                .finish()
                .map_err(|error| ApplicationError::Storage(error.to_string()))?;
        }
        Ok(DirtySnapshot {
            tracked_patch,
            untracked_archive: archive_buffer.into_inner(),
            untracked_paths: facts.untracked_paths,
        })
    }

    async fn create_worktree(
        &self,
        repository: &Path,
        run_id: RunId,
        candidate: Option<&CandidateId>,
        role: WorktreeRole,
        baseline: &CommitHash,
        branch: &BranchName,
    ) -> ApplicationResult<WorktreeHandle> {
        let path = match (role, candidate) {
            (WorktreeRole::Candidate, Some(candidate)) => {
                self.layout.candidate_worktree(run_id, candidate)
            }
            (WorktreeRole::Integration, _) => self.layout.integration_worktree(run_id),
            _ => {
                return Err(ApplicationError::Internal(
                    "only candidate and integration worktrees may be created".to_string(),
                ))
            }
        };
        if path.join(".git").exists() {
            return Ok(WorktreeHandle {
                path,
                branch: branch.clone(),
                role,
                baseline: baseline.clone(),
                reused: true,
            });
        }
        if let Some(parent) = path.parent() {
            ensure_directory(parent)?;
        }
        let path_text = path.display().to_string();
        let outcome = self
            .git(
                repository,
                &[
                    "worktree",
                    "add",
                    "--force",
                    "-B",
                    branch.as_str(),
                    &path_text,
                    baseline.as_str(),
                ],
            )
            .await?;
        if !outcome.succeeded() {
            return Err(ApplicationError::Git(format!(
                "could not create the worktree at `{path_text}`: {}",
                outcome.stderr_text().trim()
            )));
        }
        Ok(WorktreeHandle {
            path,
            branch: branch.clone(),
            role,
            baseline: baseline.clone(),
            reused: false,
        })
    }

    async fn apply_snapshot(
        &self,
        worktree: &Path,
        snapshot: &DirtySnapshot,
    ) -> ApplicationResult<()> {
        if !snapshot.tracked_patch.is_empty() {
            self.apply_patch(worktree, &snapshot.tracked_patch).await?;
        }
        if snapshot.untracked_archive.is_empty() {
            return Ok(());
        }
        let cursor = Cursor::new(snapshot.untracked_archive.clone());
        let mut archive = zip::ZipArchive::new(cursor)
            .map_err(|error| ApplicationError::Storage(error.to_string()))?;
        for index in 0..archive.len() {
            let mut entry = archive
                .by_index(index)
                .map_err(|error| ApplicationError::Storage(error.to_string()))?;
            let Some(name) = entry.enclosed_name() else {
                continue;
            };
            let destination = worktree.join(&name);
            if let Some(parent) = destination.parent() {
                ensure_directory(parent)?;
            }
            let mut contents = Vec::new();
            entry
                .read_to_end(&mut contents)
                .map_err(|error| ApplicationError::Storage(error.to_string()))?;
            crate::atomic::write_atomic(&destination, &contents)?;
        }
        Ok(())
    }

    async fn diff_against_baseline(
        &self,
        worktree: &Path,
        baseline: &CommitHash,
    ) -> ApplicationResult<(Vec<u8>, DiffSummary)> {
        self.stage_all(worktree).await?;
        let patch = self
            .git_checked(
                worktree,
                &["diff", "--binary", "--cached", baseline.as_str(), "--"],
            )
            .await?
            .into_bytes();
        let numstat = self
            .git_checked(
                worktree,
                &["diff", "--numstat", "--cached", baseline.as_str(), "--"],
            )
            .await?;
        let mut summary = DiffSummary::default();
        for line in numstat.lines() {
            let mut columns = line.split('\t');
            let added = columns.next().unwrap_or("0");
            let removed = columns.next().unwrap_or("0");
            let path = columns.next().unwrap_or("").to_string();
            if path.is_empty() {
                continue;
            }
            summary.changed_files += 1;
            summary.added_lines += added.parse::<u64>().unwrap_or(0);
            summary.removed_lines += removed.parse::<u64>().unwrap_or(0);
            summary.paths.push(path);
        }
        summary.is_empty = patch.is_empty();
        Ok((patch, summary))
    }

    async fn apply_patch(&self, worktree: &Path, patch: &[u8]) -> ApplicationResult<()> {
        if patch.is_empty() {
            return Ok(());
        }
        let (path, _guard) = self.write_temporary_patch(patch)?;
        let path_text = path.display().to_string();
        let outcome = self
            .git(
                worktree,
                &["apply", "--binary", "--whitespace=nowarn", &path_text],
            )
            .await?;
        if !outcome.succeeded() {
            return Err(ApplicationError::Git(format!(
                "the patch did not apply: {}",
                outcome.stderr_text().trim()
            )));
        }
        Ok(())
    }

    async fn check_patch_applies(
        &self,
        worktree: &Path,
        patch: &[u8],
    ) -> ApplicationResult<Result<(), String>> {
        if patch.is_empty() {
            return Ok(Ok(()));
        }
        let (path, _guard) = self.write_temporary_patch(patch)?;
        let path_text = path.display().to_string();
        let outcome = self
            .git(worktree, &["apply", "--check", "--binary", &path_text])
            .await?;
        if outcome.succeeded() {
            Ok(Ok(()))
        } else {
            Ok(Err(outcome.stderr_text().trim().to_string()))
        }
    }

    async fn reset_worktree(
        &self,
        worktree: &Path,
        baseline: &CommitHash,
    ) -> ApplicationResult<()> {
        self.git_checked(worktree, &["reset", "--hard", baseline.as_str()])
            .await?;
        self.git_checked(worktree, &["clean", "-fd"]).await?;
        Ok(())
    }

    async fn remove_worktree(&self, repository: &Path, worktree: &Path) -> ApplicationResult<()> {
        let path_text = worktree.display().to_string();
        let outcome = self
            .git(repository, &["worktree", "remove", "--force", &path_text])
            .await?;
        if !outcome.succeeded() {
            crate::atomic::remove_directory(worktree)?;
        }
        let _ = self.git(repository, &["worktree", "prune"]).await?;
        Ok(())
    }

    async fn list_run_worktrees(
        &self,
        repository: &Path,
        run_id: RunId,
    ) -> ApplicationResult<Vec<PathBuf>> {
        let listing = self
            .git_checked(repository, &["worktree", "list", "--porcelain"])
            .await?;
        let marker = run_id.to_string();
        Ok(listing
            .lines()
            .filter_map(|line| line.strip_prefix("worktree "))
            .filter(|path| path.contains(&marker))
            .map(PathBuf::from)
            .collect())
    }

    async fn create_commit(&self, request: &CommitRequest) -> ApplicationResult<CommitOutcome> {
        let facts = self.inspect(&request.worktree).await?;
        let email = facts.configured_user_email.clone().ok_or_else(|| {
            ApplicationError::UserActionRequired(
                "the repository has no configured Git email, so no commit identity can be derived"
                    .to_string(),
            )
        })?;
        self.git_checked(
            &request.worktree,
            &["checkout", "-B", request.branch.as_str()],
        )
        .await?;
        self.git_checked(&request.worktree, &["reset"]).await?;

        let mut add_arguments = vec!["add".to_string(), "--".to_string()];
        add_arguments.extend(request.paths.iter().cloned());
        let add_reference: Vec<&str> = add_arguments.iter().map(String::as_str).collect();
        self.git_checked(&request.worktree, &add_reference).await?;

        let staged = self
            .git_checked(&request.worktree, &["diff", "--cached", "--name-only"])
            .await?;
        let staged_count = staged
            .lines()
            .filter(|line| !line.trim().is_empty())
            .count() as u32;
        if staged_count == 0 {
            return Err(ApplicationError::Git(
                "no paths were staged for the commit".to_string(),
            ));
        }

        let message = if request.body.trim().is_empty() {
            request.subject.clone()
        } else {
            format!("{}\n\n{}", request.subject, request.body)
        };
        let environment = vec![
            ("GIT_AUTHOR_NAME".to_string(), request.author_name.clone()),
            ("GIT_AUTHOR_EMAIL".to_string(), email.clone()),
            (
                "GIT_COMMITTER_NAME".to_string(),
                request.committer_name.clone(),
            ),
            ("GIT_COMMITTER_EMAIL".to_string(), email),
        ];
        let outcome = self
            .git_with_environment(
                &request.worktree,
                &["commit", "--message", &message],
                environment,
            )
            .await?;
        if !outcome.succeeded() {
            let detail = outcome.stderr_text();
            if detail.contains("gpg failed")
                || detail.contains("secret key")
                || detail.contains("signing")
            {
                return Err(ApplicationError::UserActionRequired(format!(
                    "commit signing could not run without interaction: {}",
                    detail.trim()
                )));
            }
            return Err(ApplicationError::Git(format!(
                "the commit failed: {}",
                detail.trim()
            )));
        }

        let commit_hash = CommitHash::from_str(
            self.git_checked(&request.worktree, &["rev-parse", "HEAD"])
                .await?
                .trim(),
        )?;
        let signed = self
            .git(
                &request.worktree,
                &["log", "-1", "--pretty=format:%G?", commit_hash.as_str()],
            )
            .await?
            .stdout_text()
            .trim()
            .starts_with(['G', 'U', 'X', 'Y', 'R']);

        Ok(CommitOutcome {
            commit_hash,
            branch: request.branch.clone(),
            signed,
            changed_files: staged_count,
        })
    }

    async fn changed_paths(
        &self,
        worktree: &Path,
        baseline: &CommitHash,
    ) -> ApplicationResult<Vec<String>> {
        self.stage_all(worktree).await?;
        let listing = self
            .git_checked(
                worktree,
                &["diff", "--name-only", "--cached", baseline.as_str(), "--"],
            )
            .await?;
        Ok(listing
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(str::to_string)
            .collect())
    }

    async fn list_paths(&self, worktree: &Path, limit: usize) -> ApplicationResult<Vec<String>> {
        let listing = self.git_checked(worktree, &["ls-files"]).await?;
        Ok(listing
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .take(limit)
            .map(str::to_string)
            .collect())
    }

    async fn file_at_commit(
        &self,
        repository: &Path,
        commit: &CommitHash,
        relative_path: &str,
    ) -> ApplicationResult<Option<Vec<u8>>> {
        let specification = format!("{}:{}", commit.as_str(), relative_path);
        let outcome = self.git(repository, &["show", &specification]).await?;
        if outcome.succeeded() {
            Ok(Some(outcome.stdout))
        } else {
            Ok(None)
        }
    }
}

impl CommandLineGitService {
    pub fn author_name(&self) -> &str {
        &self.author_name
    }
}
