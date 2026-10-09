use super::raw;
use std::os::windows::io::OwnedHandle;
use windows_sys::Win32::System::Threading::{
    GetExitCodeProcess, GetProcessHandleCount, GetProcessTimes, WaitForSingleObject,
};

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct ProcessSample {
    pub(crate) memory_available: bool,
    pub(crate) working_set: usize,
    pub(crate) private_memory: usize,
    pub(crate) peak_working_set: usize,
    pub(crate) cpu_ticks: u64,
    pub(crate) handle_count: u32,
}

pub(crate) fn process_sample(process: &OwnedHandle) -> ProcessSample {
    let memory = crate::process_memory::for_process(process);
    let mut creation = Default::default();
    let mut exit = Default::default();
    let mut kernel = Default::default();
    let mut user = Default::default();
    let times_ok = unsafe {
        GetProcessTimes(
            raw(process),
            &mut creation,
            &mut exit,
            &mut kernel,
            &mut user,
        )
    } != 0;
    let mut handles = 0_u32;
    unsafe { GetProcessHandleCount(raw(process), &mut handles) };
    ProcessSample {
        memory_available: memory.is_some(),
        working_set: memory.map_or(0, |sample| sample.working_set),
        private_memory: memory.map_or(0, |sample| sample.private),
        peak_working_set: memory.map_or(0, |sample| sample.peak_working_set),
        cpu_ticks: if times_ok {
            file_time(kernel) + file_time(user)
        } else {
            0
        },
        handle_count: handles,
    }
}

pub(crate) fn process_exited(process: &OwnedHandle) -> bool {
    (unsafe { WaitForSingleObject(raw(process), 0) })
        == windows_sys::Win32::Foundation::WAIT_OBJECT_0
}

pub(crate) fn wait_for_process(process: &OwnedHandle, timeout: std::time::Duration) -> bool {
    let millis = timeout.as_millis().min(u32::MAX as u128) as u32;
    (unsafe { WaitForSingleObject(raw(process), millis) })
        == windows_sys::Win32::Foundation::WAIT_OBJECT_0
}

pub(crate) fn exit_code(process: &OwnedHandle) -> Option<u32> {
    let mut code = 0_u32;
    (unsafe { GetExitCodeProcess(raw(process), &mut code) } != 0).then_some(code)
}

pub(crate) fn terminate_job(job: &OwnedHandle, code: u32) {
    let _ = terminate_job_checked(job, code);
}

pub(crate) fn terminate_job_checked(job: &OwnedHandle, code: u32) -> std::io::Result<()> {
    let result =
        unsafe { windows_sys::Win32::System::JobObjects::TerminateJobObject(raw(job), code) };
    if result == 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}

fn file_time(time: windows_sys::Win32::Foundation::FILETIME) -> u64 {
    (u64::from(time.dwHighDateTime) << 32) | u64::from(time.dwLowDateTime)
}
