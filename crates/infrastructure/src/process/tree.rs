use std::process::ExitStatus;

use tokio::process::{Child, Command};

#[cfg(unix)]
pub fn configure_isolated_group(command: &mut Command) {
    command.process_group(0);
}

#[cfg(windows)]
pub fn configure_isolated_group(command: &mut Command) {
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
    const CREATE_SUSPENDED: u32 = 0x0000_0004;
    let _ = CREATE_SUSPENDED;
    command.creation_flags(CREATE_NEW_PROCESS_GROUP);
}

#[cfg(unix)]
pub struct ProcessGroupHandle;

#[cfg(unix)]
pub fn register(_child: &Child) -> Option<ProcessGroupHandle> {
    Some(ProcessGroupHandle)
}

#[cfg(unix)]
pub fn request_graceful_stop(process_id: Option<u32>) {
    let Some(process_id) = process_id else {
        return;
    };
    let group = nix::unistd::Pid::from_raw(-(process_id as i32));
    let _ = nix::sys::signal::kill(group, nix::sys::signal::Signal::SIGTERM);
}

#[cfg(unix)]
pub fn terminate_group(process_id: Option<u32>, _handle: Option<ProcessGroupHandle>) -> u32 {
    let Some(process_id) = process_id else {
        return 0;
    };
    let group = nix::unistd::Pid::from_raw(-(process_id as i32));
    let mut terminated = 0;
    if nix::sys::signal::kill(group, None).is_ok() {
        let _ = nix::sys::signal::kill(group, nix::sys::signal::Signal::SIGKILL);
        terminated += 1;
    }
    terminated
}

#[cfg(unix)]
pub fn signal_of(status: &ExitStatus) -> Option<i32> {
    use std::os::unix::process::ExitStatusExt;
    status.signal()
}

#[cfg(windows)]
pub struct ProcessGroupHandle {
    handle: windows_sys::Win32::Foundation::HANDLE,
}

#[cfg(windows)]
unsafe impl Send for ProcessGroupHandle {}

#[cfg(windows)]
pub fn register(child: &Child) -> Option<ProcessGroupHandle> {
    use windows_sys::Win32::Foundation::{CloseHandle, FALSE, HANDLE};
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
        SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
        JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    };
    use windows_sys::Win32::System::Threading::{
        OpenProcess, PROCESS_SET_QUOTA, PROCESS_TERMINATE,
    };

    let process_id = child.id()?;
    unsafe {
        let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
        if job == 0 {
            return None;
        }
        let mut information: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
        information.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        SetInformationJobObject(
            job,
            JobObjectExtendedLimitInformation,
            &information as *const _ as *const core::ffi::c_void,
            std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        );
        let process: HANDLE = OpenProcess(PROCESS_SET_QUOTA | PROCESS_TERMINATE, FALSE, process_id);
        if process == 0 {
            CloseHandle(job);
            return None;
        }
        AssignProcessToJobObject(job, process);
        CloseHandle(process);
        Some(ProcessGroupHandle { handle: job })
    }
}

#[cfg(windows)]
pub fn request_graceful_stop(_process_id: Option<u32>) {}

#[cfg(windows)]
pub fn terminate_group(_process_id: Option<u32>, handle: Option<ProcessGroupHandle>) -> u32 {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::JobObjects::TerminateJobObject;
    match handle {
        Some(handle) => unsafe {
            TerminateJobObject(handle.handle, 1);
            CloseHandle(handle.handle);
            1
        },
        None => 0,
    }
}

#[cfg(windows)]
pub fn signal_of(_status: &ExitStatus) -> Option<i32> {
    None
}
