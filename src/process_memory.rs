//! Private commit, not resident working set, is the Windows Job memory charge.
//! Failed observations remain unknown; they cannot prove available headroom.

#[derive(Clone, Copy, Debug)]
pub(crate) struct Sample {
    pub(crate) working_set: usize,
    pub(crate) private: usize,
    pub(crate) peak_working_set: usize,
}

#[cfg(windows)]
pub(crate) fn current() -> Option<Sample> {
    // SAFETY: this non-owning pseudo-handle always denotes this process.
    unsafe { sample(windows_sys::Win32::System::Threading::GetCurrentProcess()) }
}

#[cfg(not(windows))]
pub(crate) fn current() -> Option<Sample> {
    None
}

#[cfg(windows)]
pub(crate) fn for_process(process: &std::os::windows::io::OwnedHandle) -> Option<Sample> {
    use std::os::windows::io::AsRawHandle;
    // SAFETY: the borrowed OwnedHandle remains live for the duration of the call.
    unsafe { sample(process.as_raw_handle()) }
}

#[cfg(windows)]
unsafe fn sample(process: windows_sys::Win32::Foundation::HANDLE) -> Option<Sample> {
    use windows_sys::Win32::System::ProcessStatus::{
        K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS, PROCESS_MEMORY_COUNTERS_EX,
    };
    let mut memory = PROCESS_MEMORY_COUNTERS_EX {
        cb: std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32,
        ..Default::default()
    };
    // SAFETY: the caller provides a live process handle; the output pointer and
    // advertised EX structure size match. No handle or output pointer is retained.
    let success = unsafe {
        K32GetProcessMemoryInfo(
            process,
            (&mut memory as *mut PROCESS_MEMORY_COUNTERS_EX).cast::<PROCESS_MEMORY_COUNTERS>(),
            memory.cb,
        )
    };
    (success != 0).then_some(Sample {
        working_set: memory.WorkingSetSize,
        private: memory.PrivateUsage,
        peak_working_set: observed_peak(memory.PeakWorkingSetSize, memory.WorkingSetSize),
    })
}

fn observed_peak(reported: usize, current: usize) -> usize {
    // The native high-water counter can briefly lag an observed working set.
    reported.max(current)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn peak_cannot_understate_a_measured_working_set() {
        for (reported, current, expected) in [
            (8192, 4096, 8192),
            (4096, 8192, 8192),
            (0, 0, 0),
            (usize::MAX, 0, usize::MAX),
        ] {
            assert_eq!(observed_peak(reported, current), expected);
        }
    }

    #[cfg(windows)]
    #[test]
    fn current_process_commit_is_observable_without_opening_another_process() {
        let sample = current().expect("GetProcessMemoryInfo for the current process");
        assert!(sample.private > 0);
        assert!(sample.peak_working_set >= sample.working_set);
    }
}
