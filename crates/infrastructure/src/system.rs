use std::path::{Path, PathBuf};

use async_trait::async_trait;
use heikas_application::error::ApplicationResult;
use heikas_application::ports::clock::{Clock, IdentifierFactory, LocalIdentity};
use heikas_application::ports::environment::{DiskSpace, HostEnvironment, HostFacts};
use heikas_domain::clock::Timestamp;
use heikas_domain::identity::{ApprovalId, EventId, RunId};
use rand::Rng;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::layout::StoreLayout;

pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Timestamp {
        Timestamp::from_offset(OffsetDateTime::now_utc())
    }
}

pub struct UuidIdentifierFactory;

impl IdentifierFactory for UuidIdentifierFactory {
    fn new_run_id(&self) -> RunId {
        RunId::from_uuid(Uuid::now_v7())
    }

    fn new_event_id(&self) -> EventId {
        EventId::from_uuid(Uuid::now_v7())
    }

    fn new_approval_id(&self) -> ApprovalId {
        ApprovalId::from_uuid(Uuid::now_v7())
    }

    fn jitter_fraction(&self) -> f64 {
        rand::rng().random_range(0.0..=1.0)
    }
}

pub struct OperatingSystemIdentity;

impl LocalIdentity for OperatingSystemIdentity {
    fn user_name(&self) -> String {
        std::env::var("HEIKAS_USER")
            .or_else(|_| std::env::var("USER"))
            .or_else(|_| std::env::var("USERNAME"))
            .unwrap_or_else(|_| "local-user".to_string())
    }
}

pub struct LocalHostEnvironment {
    layout: StoreLayout,
}

impl LocalHostEnvironment {
    pub fn new(layout: StoreLayout) -> Self {
        Self { layout }
    }
}

#[async_trait]
impl HostEnvironment for LocalHostEnvironment {
    async fn facts(&self) -> ApplicationResult<HostFacts> {
        let root = self.layout.root().to_path_buf();
        let writable = ensure_writable(&root);
        Ok(HostFacts {
            operating_system: std::env::consts::OS.to_string(),
            architecture: std::env::consts::ARCH.to_string(),
            logical_processors: num_cpus::get(),
            heikas_home: root,
            data_root_writable: writable,
        })
    }

    async fn disk_space(&self, path: &Path) -> ApplicationResult<DiskSpace> {
        Ok(available_space(path))
    }

    fn environment_variable(&self, name: &str) -> Option<String> {
        std::env::var(name).ok()
    }

    fn home_directory(&self) -> Option<PathBuf> {
        std::env::var_os("HOME")
            .or_else(|| std::env::var_os("USERPROFILE"))
            .map(PathBuf::from)
    }
}

fn ensure_writable(path: &Path) -> bool {
    if crate::atomic::ensure_directory(path).is_err() {
        return false;
    }
    let probe = path.join(".heikas-write-probe");
    match std::fs::write(&probe, b"probe") {
        Ok(()) => {
            let _ = std::fs::remove_file(&probe);
            true
        }
        Err(_) => false,
    }
}

#[cfg(unix)]
fn available_space(path: &Path) -> DiskSpace {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;

    let mut target = path;
    let existing = loop {
        if target.exists() {
            break target;
        }
        match target.parent() {
            Some(parent) => target = parent,
            None => break Path::new("/"),
        }
    };
    let Ok(raw) = CString::new(existing.as_os_str().as_bytes()) else {
        return DiskSpace {
            available_bytes: 0,
            total_bytes: 0,
        };
    };
    unsafe {
        let mut stats: StatvfsRecord = std::mem::zeroed();
        if statvfs_call(raw.as_ptr(), &mut stats) != 0 {
            return DiskSpace {
                available_bytes: 0,
                total_bytes: 0,
            };
        }
        let block = stats.f_frsize.max(1) as u64;
        DiskSpace {
            available_bytes: block.saturating_mul(stats.f_bavail as u64),
            total_bytes: block.saturating_mul(stats.f_blocks as u64),
        }
    }
}

#[cfg(unix)]
type StatvfsRecord = nix::libc::statvfs;

#[cfg(unix)]
unsafe fn statvfs_call(path: *const std::os::raw::c_char, stats: *mut StatvfsRecord) -> i32 {
    nix::libc::statvfs(path, stats)
}

#[cfg(windows)]
fn available_space(path: &Path) -> DiskSpace {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;

    let mut target = path;
    let existing = loop {
        if target.exists() {
            break target;
        }
        match target.parent() {
            Some(parent) => target = parent,
            None => break Path::new("C:\\"),
        }
    };
    let mut wide: Vec<u16> = existing.as_os_str().encode_wide().collect();
    wide.push(0);
    let mut available: u64 = 0;
    let mut total: u64 = 0;
    let succeeded = unsafe {
        GetDiskFreeSpaceExW(
            wide.as_ptr(),
            &mut available,
            &mut total,
            std::ptr::null_mut(),
        )
    };
    if succeeded == 0 {
        return DiskSpace {
            available_bytes: 0,
            total_bytes: 0,
        };
    }
    DiskSpace {
        available_bytes: available,
        total_bytes: total,
    }
}
