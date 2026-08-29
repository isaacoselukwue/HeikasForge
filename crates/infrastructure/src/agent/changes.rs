use std::collections::BTreeMap;
use std::path::Path;

use heikas_application::error::ApplicationResult;
use heikas_domain::identity::ContentDigest;
use walkdir::WalkDir;

pub type FileFingerprints = BTreeMap<String, ContentDigest>;

pub fn observe_changed_paths(worktree: &Path) -> ApplicationResult<FileFingerprints> {
    let mut fingerprints = FileFingerprints::new();
    if !worktree.exists() {
        return Ok(fingerprints);
    }
    let root = crate::paths::canonical_root(worktree)?;
    for entry in WalkDir::new(&root)
        .into_iter()
        .filter_entry(|entry| entry.file_name().to_string_lossy() != ".git")
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
    {
        let Some(relative) = crate::paths::relative_within(&root, entry.path()) else {
            continue;
        };
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        let modified = metadata
            .modified()
            .ok()
            .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        let material = format!("{}:{}:{}", relative, metadata.len(), modified);
        fingerprints.insert(relative, ContentDigest::of_str(&material));
    }
    Ok(fingerprints)
}

pub fn difference(before: &FileFingerprints, after: &FileFingerprints) -> Vec<String> {
    let mut changed = Vec::new();
    for (path, digest) in after {
        match before.get(path) {
            Some(previous) if previous == digest => {}
            _ => changed.push(path.clone()),
        }
    }
    for path in before.keys() {
        if !after.contains_key(path) {
            changed.push(path.clone());
        }
    }
    changed.sort();
    changed.dedup();
    changed
}
