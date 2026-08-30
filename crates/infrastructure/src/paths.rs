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
