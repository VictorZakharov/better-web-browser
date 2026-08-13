use super::platform::*;

pub(super) struct MemorySample {
    pub(super) working_set: usize,
    pub(super) peak_working_set: usize,
    pub(super) private_usage: usize,
}

pub(super) fn process_memory() -> MemorySample {
    unsafe {
        let mut counters: ProcessMemoryCountersEx = std::mem::zeroed();
        counters.size = size_of::<ProcessMemoryCountersEx>() as u32;
        if GetProcessMemoryInfo(GetCurrentProcess(), &mut counters, counters.size) == 0 {
            MemorySample {
                working_set: 0,
                peak_working_set: 0,
                private_usage: 0,
            }
        } else {
            MemorySample {
                working_set: counters.working_set_size,
                peak_working_set: counters.peak_working_set_size,
                private_usage: counters.private_usage,
            }
        }
    }
}

pub(super) fn process_cpu_ticks() -> Option<u64> {
    unsafe {
        let mut creation = FileTime { low: 0, high: 0 };
        let mut exit = creation;
        let mut kernel = creation;
        let mut user = creation;
        if GetProcessTimes(
            GetCurrentProcess(),
            &mut creation,
            &mut exit,
            &mut kernel,
            &mut user,
        ) == 0
        {
            None
        } else {
            Some(file_time_ticks(kernel) + file_time_ticks(user))
        }
    }
}

fn file_time_ticks(time: FileTime) -> u64 {
    ((time.high as u64) << 32) | time.low as u64
}
