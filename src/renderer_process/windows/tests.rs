use super::{PROHIBIT_DYNAMIC_CODE, renderer_mitigations};

#[test]
fn each_budget_enforces_process_and_job_commit_without_relaxing_other_limits() {
    use super::{create_renderer_job, raw};
    use crate::renderer_budget::RendererBudget;
    use std::mem::size_of;
    use windows_sys::Win32::System::JobObjects::*;
    for budget in [RendererBudget::Standard, RendererBudget::Graphics] {
        let job = create_renderer_job(budget).unwrap();
        let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        assert_ne!(
            unsafe {
                QueryInformationJobObject(
                    raw(&job),
                    JobObjectExtendedLimitInformation,
                    (&mut limits as *mut JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                    size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
                    std::ptr::null_mut(),
                )
            },
            0
        );
        assert_eq!(limits.ProcessMemoryLimit, budget.bytes());
        assert_eq!(limits.JobMemoryLimit, budget.bytes());
        assert_eq!(limits.BasicLimitInformation.ActiveProcessLimit, 1);
        let required = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
            | JOB_OBJECT_LIMIT_ACTIVE_PROCESS
            | JOB_OBJECT_LIMIT_PROCESS_MEMORY
            | JOB_OBJECT_LIMIT_JOB_MEMORY;
        assert_eq!(limits.BasicLimitInformation.LimitFlags & required, required);
    }
}

#[test]
fn production_v8_policy_permits_jit_without_dropping_other_mitigations() {
    let production = renderer_mitigations();
    let retained = 0x1
        | 0x4
        | (0x3 << 8)
        | (0x1 << 12)
        | (0x1 << 16)
        | (0x1 << 20)
        | (0x1 << 24)
        | (0x1 << 32)
        | (0x1 << 40)
        | (0x1 << 52)
        | (0x1 << 56);

    assert_eq!(production & PROHIBIT_DYNAMIC_CODE, 0);
    assert_eq!(production, retained);
}
