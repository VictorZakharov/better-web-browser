use super::raw;
use std::mem::size_of;
use std::os::windows::io::OwnedHandle;
use windows_sys::Win32::System::ProcessStatus::{
    K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS, PROCESS_MEMORY_COUNTERS_EX,
};
use windows_sys::Win32::System::Threading::{
    GetExitCodeProcess, GetProcessHandleCount, GetProcessTimes, WaitForSingleObject,
};

#[derive(Clone, Copy, Debug, Default)]
pub(in crate::renderer_process) struct ProcessSample {
    pub(in crate::renderer_process) working_set: usize,
    pub(in crate::renderer_process) private_memory: usize,
    pub(in crate::renderer_process) peak_working_set: usize,
    pub(in crate::renderer_process) cpu_ticks: u64,
    pub(in crate::renderer_process) handle_count: u32,
}

pub(in crate::renderer_process) fn process_sample(process: &OwnedHandle) -> ProcessSample {
    let mut memory = PROCESS_MEMORY_COUNTERS_EX {
        cb: size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32,
        ..Default::default()
    };
    let memory_ok = unsafe {
        K32GetProcessMemoryInfo(
            raw(process),
            (&mut memory as *mut PROCESS_MEMORY_COUNTERS_EX).cast::<PROCESS_MEMORY_COUNTERS>(),
            memory.cb,
        )
    } != 0;
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
        working_set: if memory_ok { memory.WorkingSetSize } else { 0 },
        private_memory: if memory_ok { memory.PrivateUsage } else { 0 },
        peak_working_set: if memory_ok {
            memory.PeakWorkingSetSize
        } else {
            0
        },
        cpu_ticks: if times_ok {
            file_time(kernel) + file_time(user)
        } else {
            0
        },
        handle_count: handles,
    }
}

pub(in crate::renderer_process) fn process_exited(process: &OwnedHandle) -> bool {
    (unsafe { WaitForSingleObject(raw(process), 0) })
        == windows_sys::Win32::Foundation::WAIT_OBJECT_0
}

pub(in crate::renderer_process) fn wait_for_process(
    process: &OwnedHandle,
    timeout: std::time::Duration,
) -> bool {
    let millis = timeout.as_millis().min(u32::MAX as u128) as u32;
    (unsafe { WaitForSingleObject(raw(process), millis) })
        == windows_sys::Win32::Foundation::WAIT_OBJECT_0
}

pub(in crate::renderer_process) fn exit_code(process: &OwnedHandle) -> Option<u32> {
    let mut code = 0_u32;
    (unsafe { GetExitCodeProcess(raw(process), &mut code) } != 0).then_some(code)
}

pub(in crate::renderer_process) fn terminate_job(job: &OwnedHandle, code: u32) {
    unsafe {
        windows_sys::Win32::System::JobObjects::TerminateJobObject(raw(job), code);
    }
}

fn file_time(time: windows_sys::Win32::Foundation::FILETIME) -> u64 {
    (u64::from(time.dwHighDateTime) << 32) | u64::from(time.dwLowDateTime)
}
