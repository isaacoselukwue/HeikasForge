use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use heikas_application::error::{ApplicationError, ApplicationResult};

pub fn ensure_directory(path: &Path) -> ApplicationResult<()> {
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder
        .create(path)
        .map_err(|error| storage(path, "create directory", error))
}

#[cfg(unix)]
fn restrict_file(file: &File, path: &Path) -> ApplicationResult<()> {
    use std::os::unix::fs::PermissionsExt;
    file.set_permissions(fs::Permissions::from_mode(0o600))
        .map_err(|error| storage(path, "restrict permissions on", error))
}

#[cfg(not(unix))]
fn restrict_file(_file: &File, _path: &Path) -> ApplicationResult<()> {
    Ok(())
}

pub fn write_atomic(path: &Path, bytes: &[u8]) -> ApplicationResult<()> {
    let parent = path.parent().ok_or_else(|| {
        ApplicationError::Storage(format!("`{}` has no parent directory", path.display()))
    })?;
    ensure_directory(parent)?;
    let temporary = temporary_sibling(path);
    {
        let mut file =
            File::create(&temporary).map_err(|error| storage(&temporary, "create", error))?;
        restrict_file(&file, &temporary)?;
        file.write_all(bytes)
            .map_err(|error| storage(&temporary, "write", error))?;
        file.flush()
            .map_err(|error| storage(&temporary, "flush", error))?;
        file.sync_all()
            .map_err(|error| storage(&temporary, "synchronise", error))?;
    }
    fs::rename(&temporary, path).map_err(|error| storage(path, "rename into place", error))?;
    sync_directory(parent)
}

pub fn write_atomic_json<T: serde::Serialize>(path: &Path, value: &T) -> ApplicationResult<()> {
    let bytes = serde_json::to_vec_pretty(value)
        .map_err(|error| ApplicationError::Serialisation(error.to_string()))?;
    let mut buffer = bytes;
    buffer.push(b'\n');
    write_atomic(path, &buffer)
}

pub fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> ApplicationResult<Option<T>> {
    match fs::read(path) {
        Ok(bytes) => {
            let value = serde_json::from_slice(&bytes).map_err(|error| {
                ApplicationError::Serialisation(format!(
                    "`{}` could not be decoded: {error}",
                    path.display()
                ))
            })?;
            Ok(Some(value))
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(storage(path, "read", error)),
    }
}

pub fn append_line_synchronised(path: &Path, line: &[u8]) -> ApplicationResult<()> {
    let parent = path.parent().ok_or_else(|| {
        ApplicationError::Storage(format!("`{}` has no parent directory", path.display()))
    })?;
    ensure_directory(parent)?;
    let mut options = OpenOptions::new();
    options.create(true).append(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(path)
        .map_err(|error| storage(path, "open for append", error))?;
    file.write_all(line)
        .map_err(|error| storage(path, "append", error))?;
    file.write_all(b"\n")
        .map_err(|error| storage(path, "append", error))?;
    file.flush()
        .map_err(|error| storage(path, "flush", error))?;
    file.sync_all()
        .map_err(|error| storage(path, "synchronise", error))
}

pub fn sync_directory(path: &Path) -> ApplicationResult<()> {
    match File::open(path) {
        Ok(directory) => match directory.sync_all() {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == io::ErrorKind::InvalidInput => Ok(()),
            Err(error) if error.raw_os_error() == Some(1) => Ok(()),
            Err(error) => Err(storage(path, "synchronise directory", error)),
        },
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::PermissionDenied => Ok(()),
        Err(error) => Err(storage(path, "open directory", error)),
    }
}

pub fn rename_directory_into_place(temporary: &Path, destination: &Path) -> ApplicationResult<()> {
    if destination.exists() {
        return Err(ApplicationError::Storage(format!(
            "`{}` already exists and completed evidence is never overwritten",
            destination.display()
        )));
    }
    let parent = destination.parent().ok_or_else(|| {
        ApplicationError::Storage(format!(
            "`{}` has no parent directory",
            destination.display()
        ))
    })?;
    ensure_directory(parent)?;
    sync_tree(temporary)?;
    fs::rename(temporary, destination)
        .map_err(|error| storage(destination, "rename directory into place", error))?;
    sync_directory(parent)
}

pub fn sync_tree(path: &Path) -> ApplicationResult<()> {
    if path.is_dir() {
        let entries = fs::read_dir(path).map_err(|error| storage(path, "read directory", error))?;
        for entry in entries {
            let entry = entry.map_err(|error| storage(path, "read directory entry", error))?;
            sync_tree(&entry.path())?;
        }
        return sync_directory(path);
    }
    match OpenOptions::new().read(true).write(true).open(path) {
        Ok(file) => match file.sync_all() {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == io::ErrorKind::PermissionDenied => Ok(()),
            Err(error) => Err(storage(path, "synchronise", error)),
        },
        Err(error) if error.kind() == io::ErrorKind::PermissionDenied => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(storage(path, "open for synchronisation", error)),
    }
}

pub fn temporary_sibling(path: &Path) -> PathBuf {
    let file_name = path
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| "value".to_string());
    let unique = std::process::id();
    let counter = next_counter();
    path.with_file_name(format!(".{file_name}.{unique}.{counter}.tmp"))
}

fn next_counter() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    COUNTER.fetch_add(1, Ordering::Relaxed)
}

pub fn remove_directory(path: &Path) -> ApplicationResult<()> {
    match fs::remove_dir_all(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(storage(path, "remove directory", error)),
    }
}

pub fn storage(path: &Path, action: &str, error: io::Error) -> ApplicationError {
    ApplicationError::Storage(format!("could not {action} `{}`: {error}", path.display()))
}
