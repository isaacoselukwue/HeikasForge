use std::path::{Path, PathBuf};
use std::process::Command;

use crate::error::{PolicyError, PolicyResult};

#[derive(Debug, Clone)]
pub struct TrackedRepository {
    pub root: PathBuf,
    pub tracked_files: Vec<String>,
}

impl TrackedRepository {
    pub fn discover(root: &Path) -> PolicyResult<Self> {
        let canonical =
            std::fs::canonicalize(root).map_err(|error| PolicyError::RepositoryUnreadable {
                path: root.display().to_string(),
                detail: error.to_string(),
            })?;
        let listing = run_git(&canonical, &["ls-files", "-z"])?;
        let tracked_files = listing
            .split('\0')
            .filter(|entry| !entry.is_empty())
            .map(str::to_string)
            .collect();
        Ok(Self {
            root: canonical,
            tracked_files,
        })
    }

    pub fn absolute(&self, relative: &str) -> PathBuf {
        self.root.join(relative)
    }

    pub fn is_tracked(&self, relative: &str) -> bool {
        self.tracked_files.iter().any(|entry| entry == relative)
    }

    pub fn read_text(&self, relative: &str) -> PolicyResult<Option<String>> {
        let path = self.absolute(relative);
        match std::fs::read(&path) {
            Ok(bytes) => match String::from_utf8(bytes) {
                Ok(text) => Ok(Some(text)),
                Err(_) => Ok(None),
            },
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(PolicyError::FileUnreadable {
                path: relative.to_string(),
                detail: error.to_string(),
            }),
        }
    }

    pub fn ignore_rules(&self) -> PolicyResult<Vec<String>> {
        let mut rules = Vec::new();
        for candidate in [".gitignore", ".git/info/exclude"] {
            let path = self.root.join(candidate);
            if let Ok(contents) = std::fs::read_to_string(&path) {
                rules.extend(
                    contents
                        .lines()
                        .map(str::trim)
                        .filter(|line| !line.is_empty() && !line.starts_with('#'))
                        .map(str::to_string),
                );
            }
        }
        Ok(rules)
    }

    pub fn commits(&self) -> PolicyResult<Vec<CommitRecord>> {
        let separator = "\u{1e}";
        let format = format!("--format=%H%x1f%an%x1f%ae%x1f%cn%x1f%ce%x1f%B{separator}");
        let output = run_git(&self.root, &["log", &format])?;
        Ok(output
            .split(separator)
            .map(str::trim)
            .filter(|entry| !entry.is_empty())
            .filter_map(|entry| {
                let mut parts = entry.split('\u{1f}');
                Some(CommitRecord {
                    hash: parts.next()?.trim().to_string(),
                    author_name: parts.next()?.to_string(),
                    author_email: parts.next()?.to_string(),
                    committer_name: parts.next()?.to_string(),
                    committer_email: parts.next()?.to_string(),
                    message: parts.next()?.to_string(),
                })
            })
            .collect())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitRecord {
    pub hash: String,
    pub author_name: String,
    pub author_email: String,
    pub committer_name: String,
    pub committer_email: String,
    pub message: String,
}

pub fn run_git(root: &Path, arguments: &[&str]) -> PolicyResult<String> {
    let output = Command::new("git")
        .args(arguments)
        .current_dir(root)
        .output()
        .map_err(|error| PolicyError::GitFailed {
            arguments: arguments.join(" "),
            detail: error.to_string(),
        })?;
    if !output.status.success() {
        return Err(PolicyError::GitFailed {
            arguments: arguments.join(" "),
            detail: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        });
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

pub fn path_is_tracked(root: &Path, relative: &str) -> bool {
    Command::new("git")
        .args(["ls-files", "--error-unmatch", relative])
        .current_dir(root)
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

pub fn path_is_ignored(root: &Path, relative: &str) -> bool {
    Command::new("git")
        .args(["check-ignore", "-q", relative])
        .current_dir(root)
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}
