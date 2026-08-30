use std::io::Read;
use std::path::{Component, Path, PathBuf};

use globset::{Glob, GlobSet, GlobSetBuilder};
use heikas_application::error::{ApplicationError, ApplicationResult};
use heikas_domain::path_policy::{evaluate_path, PathAccess, PathPolicy, RelativeWorkspacePath};
use heikas_domain::DomainError;

pub use heikas_domain::path_policy::GlobPatternMatcher;

pub fn build_glob_set(patterns: &[String]) -> ApplicationResult<GlobSet> {
    let mut builder = GlobSetBuilder::new();
    for pattern in patterns {
        let glob = Glob::new(pattern).map_err(|error| {
            ApplicationError::InvalidConfiguration(format!(
                "the pattern `{pattern}` is not a valid glob: {error}"
            ))
        })?;
        builder.add(glob);
    }
    builder.build().map_err(|error| {
        ApplicationError::InvalidConfiguration(format!("the glob set could not be built: {error}"))
    })
}

pub fn normalise_absolute(path: &Path) -> PathBuf {
    let mut normalised = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalised.pop();
            }
            other => normalised.push(other.as_os_str()),
        }
    }
    normalised
}

pub fn canonical_root(path: &Path) -> ApplicationResult<PathBuf> {
    match std::fs::canonicalize(path) {
        Ok(resolved) => Ok(resolved),
        Err(error) => Err(ApplicationError::Storage(format!(
            "could not resolve `{}`: {error}",
            path.display()
        ))),
    }
}

#[derive(Debug, Clone)]
pub struct ConfinedPath {
    pub relative: RelativeWorkspacePath,
    pub absolute: PathBuf,
}

pub fn confine(
    worktree_root: &Path,
    raw: &str,
    access: PathAccess,
    policy: &PathPolicy,
) -> ApplicationResult<ConfinedPath> {
    let relative = RelativeWorkspacePath::parse(raw)?;
    evaluate_path(policy, &GlobPatternMatcher, &relative, access)?;
    let root = canonical_root(worktree_root)?;
    let candidate = root.join(relative.as_str());
    let resolved = resolve_without_escape(&root, &candidate)?;
    Ok(ConfinedPath {
        relative,
        absolute: resolved,
    })
}

fn resolve_without_escape(root: &Path, candidate: &Path) -> ApplicationResult<PathBuf> {
    let existing_ancestor = nearest_existing_ancestor(candidate);
    let resolved_ancestor = canonical_root(&existing_ancestor)?;
    if !resolved_ancestor.starts_with(root) {
        return Err(ApplicationError::Domain(DomainError::PathEscapesWorktree {
            path: candidate.display().to_string(),
        }));
    }
    let suffix = candidate
        .strip_prefix(&existing_ancestor)
        .unwrap_or(Path::new(""));
    let resolved = resolved_ancestor.join(suffix);
    let normalised = normalise_absolute(&resolved);
    if !normalised.starts_with(root) {
        return Err(ApplicationError::Domain(DomainError::PathEscapesWorktree {
            path: candidate.display().to_string(),
        }));
    }
    Ok(normalised)
}

fn nearest_existing_ancestor(path: &Path) -> PathBuf {
    let mut current = path.to_path_buf();
    loop {
        if current.exists() {
            return current;
        }
        match current.parent() {
            Some(parent) if parent != current => current = parent.to_path_buf(),
            _ => return current,
        }
    }
}

pub fn relative_within(root: &Path, absolute: &Path) -> Option<String> {
    absolute
        .strip_prefix(root)
        .ok()
        .map(|relative| relative.to_string_lossy().replace('\\', "/"))
}

pub const MAXIMUM_REPOSITORY_CONFIGURATION_BYTES: u64 = 65_536;
pub const MAXIMUM_REPOSITORY_REPORT_BYTES: u64 = 16_777_216;

pub fn read_confined_file(
    root: &Path,
    relative: &str,
    maximum_bytes: u64,
) -> ApplicationResult<Option<Vec<u8>>> {
    let parsed = RelativeWorkspacePath::parse(relative)?;
    let resolved_root = canonical_root(root)?;
    let candidate = resolved_root.join(parsed.as_str());

    let metadata = match std::fs::symlink_metadata(&candidate) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(crate::atomic::storage(&candidate, "inspect", error)),
    };
    if metadata.file_type().is_symlink() {
        return Err(ApplicationError::PolicyViolation(format!(
            "`{relative}` is a symbolic link, which is never followed when reading from a repository"
        )));
    }
    if !metadata.is_file() {
        return Err(ApplicationError::PolicyViolation(format!(
            "`{relative}` is not a regular file, so it is not read"
        )));
    }
    if metadata.len() > maximum_bytes {
        return Err(ApplicationError::PolicyViolation(format!(
            "`{relative}` is {} bytes, which exceeds the {maximum_bytes} byte limit for a repository file",
            metadata.len()
        )));
    }

    let resolved = canonical_root(&candidate)?;
    if !resolved.starts_with(&resolved_root) {
        return Err(ApplicationError::Domain(DomainError::PathEscapesWorktree {
            path: relative.to_string(),
        }));
    }

    let mut file = std::fs::File::open(&resolved)
        .map_err(|error| crate::atomic::storage(&resolved, "open", error))?;
    let mut contents = Vec::new();
    std::io::Read::by_ref(&mut file)
        .take(maximum_bytes.saturating_add(1))
        .read_to_end(&mut contents)
        .map_err(|error| crate::atomic::storage(&resolved, "read", error))?;
    if contents.len() as u64 > maximum_bytes {
        return Err(ApplicationError::PolicyViolation(format!(
            "`{relative}` grew beyond the {maximum_bytes} byte limit while it was being read"
        )));
    }
    Ok(Some(contents))
}

pub fn confined_working_directory(
    worktree: &Path,
    subdirectory: Option<&str>,
) -> ApplicationResult<PathBuf> {
    let resolved_root = canonical_root(worktree)?;
    let Some(subdirectory) = subdirectory else {
        return Ok(resolved_root);
    };
    let parsed = RelativeWorkspacePath::parse(subdirectory)?;
    let candidate = resolved_root.join(parsed.as_str());
    if std::fs::symlink_metadata(&candidate)
        .map(|metadata| metadata.file_type().is_symlink())
        .unwrap_or(false)
    {
        return Err(ApplicationError::PolicyViolation(format!(
            "the working subdirectory `{subdirectory}` is a symbolic link, which may not be entered"
        )));
    }
    let resolved = resolve_without_escape(&resolved_root, &candidate)?;
    if !resolved.is_dir() {
        return Err(ApplicationError::InvalidConfiguration(format!(
            "the working subdirectory `{subdirectory}` does not exist in the worktree"
        )));
    }
    Ok(resolved)
}
