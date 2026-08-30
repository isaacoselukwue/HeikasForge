use std::collections::BTreeMap;
use std::fs::File;
use std::io::{BufReader, Read};
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
        let Ok(digest) = content_digest(entry.path()) else {
            continue;
        };
        fingerprints.insert(relative, digest);
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

fn content_digest(path: &Path) -> std::io::Result<ContentDigest> {
    let file = File::open(path)?;
    let mut reader = BufReader::new(file);
    let mut hasher = blake3::Hasher::new();
    let mut buffer = [0u8; 65_536];
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(ContentDigest::of_str(hasher.finalize().to_hex().as_str()))
}
