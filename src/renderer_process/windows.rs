mod app_container;
mod metrics;

pub(crate) use app_container::AppContainerSid;
pub(crate) use metrics::{
    ProcessSample, exit_code, process_exited, process_sample, terminate_job, terminate_job_checked,
    wait_for_process,
};

use crate::limits::{MEDIA_PROCESS_MEMORY_LIMIT_BYTES, RENDERER_MEMORY_LIMIT_BYTES};
use crate::renderer_protocol::Nonce;
use std::io;
use std::mem::size_of;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle, RawHandle};
use std::ptr::{null, null_mut};
use windows_sys::Win32::Foundation::{HANDLE, HANDLE_FLAG_INHERIT, SetHandleInformation};
use windows_sys::Win32::Security::Cryptography::{
    BCRYPT_USE_SYSTEM_PREFERRED_RNG, BCryptGenRandom,
};
use windows_sys::Win32::Security::{SECURITY_ATTRIBUTES, SECURITY_CAPABILITIES};
use windows_sys::Win32::System::JobObjects::{
    CreateJobObjectW, JOB_OBJECT_LIMIT_ACTIVE_PROCESS, JOB_OBJECT_LIMIT_JOB_MEMORY,
    JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE, JOB_OBJECT_LIMIT_PROCESS_MEMORY,
    JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
    SetInformationJobObject,
};
use windows_sys::Win32::System::Pipes::CreatePipe;
use windows_sys::Win32::System::Threading::{
    DeleteProcThreadAttributeList, InitializeProcThreadAttributeList, LPPROC_THREAD_ATTRIBUTE_LIST,
    PROC_THREAD_ATTRIBUTE_CHILD_PROCESS_POLICY, PROC_THREAD_ATTRIBUTE_HANDLE_LIST,
    PROC_THREAD_ATTRIBUTE_JOB_LIST, PROC_THREAD_ATTRIBUTE_MITIGATION_POLICY,
    PROC_THREAD_ATTRIBUTE_SECURITY_CAPABILITIES, UpdateProcThreadAttribute,
};
use windows_sys::Win32::System::WindowsProgramming::PROCESS_CREATION_CHILD_PROCESS_RESTRICTED;

pub(crate) fn last_error(operation: &str) -> String {
    format!("{operation}: {}", io::Error::last_os_error())
}

pub(crate) fn random_nonce() -> Result<Nonce, String> {
    let mut bytes = [0_u8; 32];
    let status = unsafe {
        BCryptGenRandom(
            null_mut(),
            bytes.as_mut_ptr(),
            bytes.len() as u32,
            BCRYPT_USE_SYSTEM_PREFERRED_RNG,
        )
    };
    if status < 0 {
        Err(format!("generate renderer nonce: NTSTATUS {status:#x}"))
    } else {
        Ok(Nonce::new(bytes))
    }
}

pub(crate) struct PipeSet {
    pub(crate) child_input: OwnedHandle,
    pub(crate) child_output: OwnedHandle,
    pub(crate) browser_input: OwnedHandle,
    pub(crate) browser_output: OwnedHandle,
}

pub(crate) struct InheritedInputPipe {
    pub(crate) child_input: OwnedHandle,
    pub(crate) browser_output: OwnedHandle,
}

pub(crate) struct InheritedOutputPipe {
    pub(crate) child_output: OwnedHandle,
    pub(crate) browser_input: OwnedHandle,
}

impl InheritedOutputPipe {
    pub(crate) fn create(role: &str) -> Result<Self, String> {
        let (browser_input, child_output) = create_pipe(&format!("create {role} pipe"))?;
        clear_inherit(&browser_input)?;
        Ok(Self {
            child_output,
            browser_input,
        })
    }
}

impl InheritedInputPipe {
    pub(crate) fn create(role: &str) -> Result<Self, String> {
        let (child_input, browser_output) = create_pipe(&format!("create {role} pipe"))?;
        clear_inherit(&browser_output)?;
        Ok(Self {
            child_input,
            browser_output,
        })
    }
}

impl PipeSet {
    pub(crate) fn create() -> Result<Self, String> {
        let (child_input, browser_output) = create_pipe("create browser-to-renderer pipe")?;
        let (browser_input, child_output) = create_pipe("create renderer-to-browser pipe")?;
        clear_inherit(&browser_output)?;
        clear_inherit(&browser_input)?;
        Ok(Self {
            child_input,
            child_output,
            browser_input,
            browser_output,
        })
    }
}

fn create_pipe(operation: &str) -> Result<(OwnedHandle, OwnedHandle), String> {
    let mut read = null_mut();
    let mut write = null_mut();
    let attributes = SECURITY_ATTRIBUTES {
        nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: null_mut(),
        bInheritHandle: 1,
    };
    if unsafe { CreatePipe(&mut read, &mut write, &attributes, 0) } == 0 {
        return Err(last_error(operation));
    }
    // SAFETY: CreatePipe returned two distinct owned handles on success.
    Ok(unsafe { (owned(read), owned(write)) })
}

fn clear_inherit(handle: &OwnedHandle) -> Result<(), String> {
    set_handle_inheritance(raw(handle), false)
}

pub(crate) fn set_handle_inheritance(handle: HANDLE, inheritable: bool) -> Result<(), String> {
    let flags = if inheritable { HANDLE_FLAG_INHERIT } else { 0 };
    if unsafe { SetHandleInformation(handle, HANDLE_FLAG_INHERIT, flags) } == 0 {
        Err(last_error("update pipe handle inheritance"))
    } else {
        Ok(())
    }
}

pub(super) fn create_renderer_job() -> Result<OwnedHandle, String> {
    create_contained_job(RENDERER_MEMORY_LIMIT_BYTES, "renderer")
}

pub(crate) fn create_media_job() -> Result<OwnedHandle, String> {
    create_contained_job(MEDIA_PROCESS_MEMORY_LIMIT_BYTES, "media worker")
}

fn create_contained_job(memory_limit: usize, role: &str) -> Result<OwnedHandle, String> {
    let handle = unsafe { CreateJobObjectW(null(), null()) };
    if handle.is_null() {
        return Err(last_error(&format!("create {role} Job Object")));
    }
    // SAFETY: CreateJobObjectW returned a new owned handle.
    let handle = unsafe { owned(handle) };
    let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
    limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
        | JOB_OBJECT_LIMIT_ACTIVE_PROCESS
        | JOB_OBJECT_LIMIT_PROCESS_MEMORY
        | JOB_OBJECT_LIMIT_JOB_MEMORY;
    limits.BasicLimitInformation.ActiveProcessLimit = 1;
    limits.ProcessMemoryLimit = memory_limit;
    limits.JobMemoryLimit = memory_limit;
    let result = unsafe {
        SetInformationJobObject(
            raw(&handle),
            JobObjectExtendedLimitInformation,
            (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
            size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        )
    };
    if result == 0 {
        Err(last_error(&format!("apply {role} Job Object limits")))
    } else {
        Ok(handle)
    }
}

pub(crate) struct LaunchAttributes {
    list: AttributeList,
    _handles: Box<[HANDLE]>,
    _jobs: Box<[HANDLE; 1]>,
    _child_policy: Box<u32>,
    _mitigations: Box<u64>,
    _security: Box<SECURITY_CAPABILITIES>,
}

impl LaunchAttributes {
    pub(crate) fn with_inherited(
        child_input: &OwnedHandle,
        child_output: &OwnedHandle,
        additional: &[HANDLE],
        job: &OwnedHandle,
        sid: &AppContainerSid,
    ) -> Result<Self, String> {
        let mut handles = Vec::with_capacity(2 + additional.len());
        handles.push(raw(child_input));
        handles.push(raw(child_output));
        handles.extend_from_slice(additional);
        let handles = handles.into_boxed_slice();
        let jobs = Box::new([raw(job)]);
        let child_policy = Box::new(PROCESS_CREATION_CHILD_PROCESS_RESTRICTED);
        let mitigations = Box::new(renderer_mitigations());
        let security = Box::new(sid.security_capabilities());
        let mut list = AttributeList::new(5)?;
        list.update(
            PROC_THREAD_ATTRIBUTE_HANDLE_LIST,
            handles.as_ptr(),
            size_of_val(&*handles),
        )?;
        list.update(
            PROC_THREAD_ATTRIBUTE_JOB_LIST,
            jobs.as_ptr(),
            size_of_val(&*jobs),
        )?;
        list.update(
            PROC_THREAD_ATTRIBUTE_CHILD_PROCESS_POLICY,
            (&*child_policy) as *const u32,
            size_of::<u32>(),
        )?;
        list.update(
            PROC_THREAD_ATTRIBUTE_MITIGATION_POLICY,
            (&*mitigations) as *const u64,
            size_of::<u64>(),
        )?;
        list.update(
            PROC_THREAD_ATTRIBUTE_SECURITY_CAPABILITIES,
            (&*security) as *const SECURITY_CAPABILITIES,
            size_of::<SECURITY_CAPABILITIES>(),
        )?;
        Ok(Self {
            list,
            _handles: handles,
            _jobs: jobs,
            _child_policy: child_policy,
            _mitigations: mitigations,
            _security: security,
        })
    }

    pub(crate) fn as_ptr(&self) -> LPPROC_THREAD_ATTRIBUTE_LIST {
        self.list.pointer
    }
}

struct AttributeList {
    storage: Vec<usize>,
    pointer: LPPROC_THREAD_ATTRIBUTE_LIST,
}

impl AttributeList {
    fn new(count: u32) -> Result<Self, String> {
        let mut bytes = 0_usize;
        unsafe {
            InitializeProcThreadAttributeList(null_mut(), count, 0, &mut bytes);
        }
        if bytes == 0 {
            return Err(last_error("size process attribute list"));
        }
        let words = bytes.div_ceil(size_of::<usize>());
        let mut storage = vec![0_usize; words];
        let pointer = storage.as_mut_ptr().cast();
        if unsafe { InitializeProcThreadAttributeList(pointer, count, 0, &mut bytes) } == 0 {
            return Err(last_error("initialize process attribute list"));
        }
        Ok(Self { storage, pointer })
    }

    fn update<T>(&mut self, attribute: u32, value: *const T, bytes: usize) -> Result<(), String> {
        let result = unsafe {
            UpdateProcThreadAttribute(
                self.pointer,
                0,
                attribute as usize,
                value.cast(),
                bytes,
                null_mut(),
                null(),
            )
        };
        if result == 0 {
            Err(last_error(&format!("set process attribute {attribute:#x}")))
        } else {
            Ok(())
        }
    }
}

impl Drop for AttributeList {
    fn drop(&mut self) {
        if !self.pointer.is_null() {
            unsafe { DeleteProcThreadAttributeList(self.pointer) };
        }
        let _ = self.storage.len();
    }
}

// Values are from the PROC_THREAD_ATTRIBUTE_MITIGATION_POLICY table. V8 requires executable JIT
// pages, so dynamic-code prohibition is intentionally absent inside the capability-free
// AppContainer renderer. Win32k is not disabled until the renderer no longer needs the current
// GDI/font bridge (ADR 0001).
#[cfg(test)]
const PROHIBIT_DYNAMIC_CODE: u64 = 0x1 << 36;

fn renderer_mitigations() -> u64 {
    0x1 // DEP
        | 0x4 // SEHOP
        | (0x3 << 8) // force relocation and require relocations
        | (0x1 << 12) // terminate on heap corruption
        | (0x1 << 16) // bottom-up ASLR
        | (0x1 << 20) // high-entropy ASLR
        | (0x1 << 24) // strict handle checks
        | (0x1 << 32) // disable extension points
        | (0x1 << 40) // enable Control Flow Guard
        | (0x1 << 52) // prohibit remote image loads
        | (0x1 << 56) // prohibit low-integrity image loads
}

pub(crate) fn raw(handle: &OwnedHandle) -> HANDLE {
    handle.as_raw_handle() as HANDLE
}

unsafe fn owned(handle: HANDLE) -> OwnedHandle {
    unsafe { OwnedHandle::from_raw_handle(handle as RawHandle) }
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}

#[cfg(test)]
mod tests;
