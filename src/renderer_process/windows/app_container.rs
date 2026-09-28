use super::{last_error, owned, raw, wide};
use std::os::windows::io::OwnedHandle;
use std::ptr::{null, null_mut};
use windows_sys::Win32::Foundation::{LocalFree, WAIT_ABANDONED, WAIT_OBJECT_0, WAIT_TIMEOUT};
use windows_sys::Win32::Security::Isolation::{
    CreateAppContainerProfile, DeleteAppContainerProfile, DeriveAppContainerSidFromAppContainerName,
};
use windows_sys::Win32::Security::{
    DeriveCapabilitySidsFromName, FreeSid, PSID, SECURITY_CAPABILITIES,
};
use windows_sys::Win32::System::Threading::{CreateMutexW, ReleaseMutex, WaitForSingleObject};

const RENDERER_PROFILE: AppContainerProfile = AppContainerProfile {
    name: "VictorZakharov.Breeze.Renderer",
    display: "Breeze renderer",
    description: "Capability-free renderer process for Breeze",
    mutex: "Local\\VictorZakharov.Breeze.Renderer.ProfileCreation",
};
const MEDIA_PROFILE: AppContainerProfile = AppContainerProfile {
    name: "VictorZakharov.Breeze.Media",
    display: "Breeze media worker",
    description: "Capability-free media process for Breeze",
    mutex: "Local\\VictorZakharov.Breeze.Media.ProfileCreation",
};
#[allow(dead_code)] // Reserved for browser-authorized capture sessions.
const CAPTURE_PROFILE: AppContainerProfile = AppContainerProfile {
    name: "VictorZakharov.Breeze.Capture",
    display: "Breeze capture worker",
    description: "Browser-controlled camera and microphone capture process for Breeze",
    mutex: "Local\\VictorZakharov.Breeze.Capture.ProfileCreation",
};
const PROFILE_CREATION_TIMEOUT_MS: u32 = 10_000;
const HRESULT_ALREADY_EXISTS: i32 = 0x8007_00B7_u32 as i32;
const HRESULT_FILE_NOT_FOUND: i32 = 0x8007_0002_u32 as i32;

#[derive(Clone, Copy)]
struct AppContainerProfile {
    name: &'static str,
    display: &'static str,
    description: &'static str,
    mutex: &'static str,
}

pub(crate) struct AppContainerSid(PSID);

impl AppContainerSid {
    pub(crate) fn create_renderer() -> Result<Self, String> {
        Self::create_or_open(RENDERER_PROFILE)
    }

    pub(crate) fn create_media() -> Result<Self, String> {
        Self::create_or_open(MEDIA_PROFILE)
    }

    #[allow(dead_code)]
    pub(crate) fn create_capture() -> Result<Self, String> {
        Self::create_or_open(CAPTURE_PROFILE)
    }

    fn create_or_open(profile: AppContainerProfile) -> Result<Self, String> {
        // Profile creation is per user, while several browser processes may start together. Keep
        // stale-profile recovery from deleting a profile that another process is still creating.
        let _profile_creation_lock = ProfileCreationLock::acquire(profile.mutex)?;
        let name = wide(profile.name);
        let display = wide(profile.display);
        let description = wide(profile.description);
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
                "create {} AppContainer profile: create HRESULT {create_result:#x}, final HRESULT {result:#x}",
                profile.display
            ))
        } else {
            Ok(Self(sid))
        }
    }

    pub(crate) fn security_capabilities(&self) -> SECURITY_CAPABILITIES {
        SECURITY_CAPABILITIES {
            AppContainerSid: self.0,
            Capabilities: null_mut(),
            CapabilityCount: 0,
            Reserved: 0,
        }
    }
}

/// `DeriveCapabilitySidsFromName` allocates both the SID and its array with LocalAlloc. The
/// capability SID must stay alive until CreateProcessW has copied SECURITY_CAPABILITIES.
pub(crate) struct CapabilitySid(PSID);

#[allow(dead_code)]
impl CapabilitySid {
    pub(crate) fn derive(name: &'static str) -> Result<Self, String> {
        let wide_name = wide(name);
        let mut groups: *mut PSID = null_mut();
        let mut group_count = 0_u32;
        let mut capabilities: *mut PSID = null_mut();
        let mut capability_count = 0_u32;
        let result = unsafe {
            DeriveCapabilitySidsFromName(
                wide_name.as_ptr(),
                &mut groups,
                &mut group_count,
                &mut capabilities,
                &mut capability_count,
            )
        };
        let failure = (result == 0).then(|| last_error(&format!("derive {name} capability SID")));
        let selected = if result != 0 && capability_count == 1 && !capabilities.is_null() {
            let sid = unsafe { *capabilities };
            unsafe { *capabilities = null_mut() };
            sid
        } else {
            null_mut()
        };
        free_sid_array(groups, group_count);
        free_sid_array(capabilities, capability_count);
        if selected.is_null() {
            return Err(failure
                .unwrap_or_else(|| format!("derive {name} capability SID: unexpected SID count")));
        }
        Ok(Self(selected))
    }

    pub(crate) fn as_ptr(&self) -> PSID {
        self.0
    }
}

impl Drop for CapabilitySid {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { LocalFree(self.0.cast()) };
        }
    }
}

#[allow(dead_code)]
fn free_sid_array(array: *mut PSID, count: u32) {
    if array.is_null() {
        return;
    }
    for sid in unsafe { std::slice::from_raw_parts(array, count as usize) } {
        if !sid.is_null() {
            unsafe { LocalFree((*sid).cast()) };
        }
    }
    unsafe { LocalFree(array.cast()) };
}

impl Drop for AppContainerSid {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { FreeSid(self.0) };
        }
    }
}

struct ProfileCreationLock {
    handle: OwnedHandle,
}

impl ProfileCreationLock {
    fn acquire(mutex: &str) -> Result<Self, String> {
        let name = wide(mutex);
        let handle = unsafe { CreateMutexW(null(), 0, name.as_ptr()) };
        if handle.is_null() {
            return Err(last_error("create AppContainer profile mutex"));
        }
        let handle = unsafe { owned(handle) };
        match unsafe { WaitForSingleObject(raw(&handle), PROFILE_CREATION_TIMEOUT_MS) } {
            WAIT_OBJECT_0 | WAIT_ABANDONED => Ok(Self { handle }),
            WAIT_TIMEOUT => Err("wait for AppContainer profile creation: timed out".into()),
            _ => Err(last_error("wait for AppContainer profile creation")),
        }
    }
}

impl Drop for ProfileCreationLock {
    fn drop(&mut self) {
        unsafe { ReleaseMutex(raw(&self.handle)) };
    }
}
