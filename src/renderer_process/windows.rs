mod metrics;

pub(super) use metrics::{
    ProcessSample, exit_code, process_exited, process_sample, terminate_job, wait_for_process,
};

use crate::limits::RENDERER_MEMORY_LIMIT_BYTES;
use crate::renderer_protocol::Nonce;
use std::io;
use std::mem::size_of;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle, RawHandle};
use std::ptr::{null, null_mut};
use windows_sys::Win32::Foundation::{HANDLE, HANDLE_FLAG_INHERIT, SetHandleInformation};
use windows_sys::Win32::Security::Cryptography::{
    BCRYPT_USE_SYSTEM_PREFERRED_RNG, BCryptGenRandom,
};
use windows_sys::Win32::Security::Isolation::{
    CreateAppContainerProfile, DeleteAppContainerProfile, DeriveAppContainerSidFromAppContainerName,
};
use windows_sys::Win32::Security::{FreeSid, PSID, SECURITY_ATTRIBUTES, SECURITY_CAPABILITIES};
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

const PROFILE_NAME: &str = "VictorZakharov.Breeze.Renderer";
const PROFILE_DISPLAY_NAME: &str = "Breeze renderer";
const PROFILE_DESCRIPTION: &str = "Capability-free renderer process for Breeze";
const HRESULT_ALREADY_EXISTS: i32 = 0x8007_00B7_u32 as i32;
const HRESULT_FILE_NOT_FOUND: i32 = 0x8007_0002_u32 as i32;

pub(super) fn last_error(operation: &str) -> String {
    format!("{operation}: {}", io::Error::last_os_error())
}

pub(super) fn random_nonce() -> Result<Nonce, String> {
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

pub(super) struct AppContainerSid(PSID);

impl AppContainerSid {
    pub(super) fn create_or_open() -> Result<Self, String> {
        let name = wide(PROFILE_NAME);
        let display = wide(PROFILE_DISPLAY_NAME);
        let description = wide(PROFILE_DESCRIPTION);
        let mut sid = null_mut();
        let create_result = unsafe {
            CreateAppContainerProfile(
                name.as_ptr(),
                display.as_ptr(),
                description.as_ptr(),
                null(),
                0,
                &mut sid,
            )
        };
        let result = if create_result == HRESULT_ALREADY_EXISTS {
            let derived =
                unsafe { DeriveAppContainerSidFromAppContainerName(name.as_ptr(), &mut sid) };
            if derived == HRESULT_FILE_NOT_FOUND {
                // A cancelled install can leave the per-user profile registration without its
                // backing store. Only this product-owned profile is removed and recreated.
                unsafe { DeleteAppContainerProfile(name.as_ptr()) };
                unsafe {
                    CreateAppContainerProfile(
                        name.as_ptr(),
                        display.as_ptr(),
                        description.as_ptr(),
                        null(),
                        0,
                        &mut sid,
                    )
                }
            } else {
                derived
            }
        } else {
            create_result
        };
        if result < 0 || sid.is_null() {
            Err(format!(
                "create renderer AppContainer profile: create HRESULT {create_result:#x}, final HRESULT {result:#x}"
            ))
        } else {
            Ok(Self(sid))
        }
    }

    pub(super) fn security_capabilities(&self) -> SECURITY_CAPABILITIES {
        SECURITY_CAPABILITIES {
            AppContainerSid: self.0,
            Capabilities: null_mut(),
            CapabilityCount: 0,
            Reserved: 0,
        }
    }
}

impl Drop for AppContainerSid {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                FreeSid(self.0);
            }
        }
    }
}

pub(super) struct PipeSet {
    pub(super) child_input: OwnedHandle,
    pub(super) child_output: OwnedHandle,
    pub(super) browser_input: OwnedHandle,
    pub(super) browser_output: OwnedHandle,
}

impl PipeSet {
    pub(super) fn create() -> Result<Self, String> {
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
    if unsafe { SetHandleInformation(raw(handle), HANDLE_FLAG_INHERIT, 0) } == 0 {
        Err(last_error("clear parent pipe inheritance"))
    } else {
        Ok(())
    }
}

pub(super) fn create_renderer_job() -> Result<OwnedHandle, String> {
    let handle = unsafe { CreateJobObjectW(null(), null()) };
    if handle.is_null() {
        return Err(last_error("create renderer Job Object"));
    }
    // SAFETY: CreateJobObjectW returned a new owned handle.
    let handle = unsafe { owned(handle) };
    let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
    limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
        | JOB_OBJECT_LIMIT_ACTIVE_PROCESS
        | JOB_OBJECT_LIMIT_PROCESS_MEMORY
        | JOB_OBJECT_LIMIT_JOB_MEMORY;
    limits.BasicLimitInformation.ActiveProcessLimit = 1;
    limits.ProcessMemoryLimit = RENDERER_MEMORY_LIMIT_BYTES;
    limits.JobMemoryLimit = RENDERER_MEMORY_LIMIT_BYTES;
    let result = unsafe {
        SetInformationJobObject(
            raw(&handle),
            JobObjectExtendedLimitInformation,
            (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
            size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        )
    };
    if result == 0 {
        Err(last_error("apply renderer Job Object limits"))
    } else {
        Ok(handle)
    }
}

pub(super) struct LaunchAttributes {
    list: AttributeList,
    _handles: Box<[HANDLE; 2]>,
    _jobs: Box<[HANDLE; 1]>,
    _child_policy: Box<u32>,
    _mitigations: Box<u64>,
    _security: Box<SECURITY_CAPABILITIES>,
}

impl LaunchAttributes {
    pub(super) fn new(
        child_input: &OwnedHandle,
        child_output: &OwnedHandle,
        job: &OwnedHandle,
        sid: &AppContainerSid,
    ) -> Result<Self, String> {
        let handles = Box::new([raw(child_input), raw(child_output)]);
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

    pub(super) fn as_ptr(&self) -> LPPROC_THREAD_ATTRIBUTE_LIST {
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

// Values are from the PROC_THREAD_ATTRIBUTE_MITIGATION_POLICY table. Win32k is intentionally not
// disabled until the renderer no longer needs the current GDI/font bridge (ADR 0001).
fn renderer_mitigations() -> u64 {
    0x1 // DEP
        | 0x4 // SEHOP
        | (0x3 << 8) // force relocation and require relocations
        | (0x1 << 12) // terminate on heap corruption
        | (0x1 << 16) // bottom-up ASLR
        | (0x1 << 20) // high-entropy ASLR
        | (0x1 << 24) // strict handle checks
        | (0x1 << 32) // disable extension points
        | (0x1 << 36) // prohibit dynamic code
        | (0x1 << 40) // enable Control Flow Guard
        | (0x1 << 52) // prohibit remote image loads
        | (0x1 << 56) // prohibit low-integrity image loads
}

pub(super) fn raw(handle: &OwnedHandle) -> HANDLE {
    handle.as_raw_handle() as HANDLE
}

unsafe fn owned(handle: HANDLE) -> OwnedHandle {
    unsafe { OwnedHandle::from_raw_handle(handle as RawHandle) }
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}
