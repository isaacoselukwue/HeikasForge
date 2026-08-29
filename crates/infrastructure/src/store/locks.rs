use std::fs::{File, OpenOptions};
use std::path::PathBuf;
use std::time::Duration;

use async_trait::async_trait;
use fs4::fs_std::FileExt;
use heikas_application::error::{ApplicationError, ApplicationResult};
use heikas_application::ports::store::{RunLockGuard, RunLockService};
use heikas_domain::identity::RunId;

use crate::atomic::{ensure_directory, storage};
use crate::layout::StoreLayout;

const ACQUIRE_ATTEMPTS: u32 = 60;
const ACQUIRE_INTERVAL: Duration = Duration::from_millis(250);

pub struct FileRunLocks {
    layout: StoreLayout,
}

impl FileRunLocks {
    pub fn new(layout: StoreLayout) -> Self {
        Self { layout }
    }

    fn open_lock_file(&self, run_id: RunId) -> ApplicationResult<(File, PathBuf)> {
        let path = self.layout.dispatcher_lock(run_id);
        if let Some(parent) = path.parent() {
            ensure_directory(parent)?;
        }
        let file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(&path)
            .map_err(|error| storage(&path, "open lock file", error))?;
        Ok((file, path))
    }
}

#[async_trait]
impl RunLockService for FileRunLocks {
    async fn acquire(&self, run_id: RunId) -> ApplicationResult<Box<dyn RunLockGuard>> {
        let (file, path) = self.open_lock_file(run_id)?;
        for _ in 0..ACQUIRE_ATTEMPTS {
            match file.try_lock_exclusive() {
                Ok(true) => {
                    return Ok(Box::new(FileRunLockGuard {
                        run_id,
                        file: Some(file),
                        path,
                    }))
                }
                Ok(false) => tokio::time::sleep(ACQUIRE_INTERVAL).await,
                Err(error) => return Err(storage(&path, "lock", error)),
            }
        }
        Err(ApplicationError::RunLocked(run_id))
    }

    async fn is_locked(&self, run_id: RunId) -> ApplicationResult<bool> {
        let (file, path) = self.open_lock_file(run_id)?;
        match file.try_lock_exclusive() {
            Ok(true) => {
                FileExt::unlock(&file).map_err(|error| storage(&path, "unlock", error))?;
                Ok(false)
            }
            Ok(false) => Ok(true),
            Err(error) => Err(storage(&path, "probe lock", error)),
        }
    }
}

pub struct FileRunLockGuard {
    run_id: RunId,
    file: Option<File>,
    path: PathBuf,
}

impl RunLockGuard for FileRunLockGuard {
    fn run_id(&self) -> RunId {
        self.run_id
    }

    fn release(mut self: Box<Self>) {
        if let Some(file) = self.file.take() {
            let _ = FileExt::unlock(&file);
        }
        let _ = &self.path;
    }
}

impl Drop for FileRunLockGuard {
    fn drop(&mut self) {
        if let Some(file) = self.file.take() {
            let _ = FileExt::unlock(&file);
        }
    }
}
