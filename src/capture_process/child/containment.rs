//! The capture child verifies its actual token before activating hardware.

use crate::capture_protocol::{CaptureContainmentReport, CaptureDevices};
use crate::renderer_process::windows::CapabilitySid;
use std::mem::{offset_of, size_of};
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle, RawHandle};
use std::ptr::null_mut;
use windows_sys::Win32::Security::{
    EqualSid, GetTokenInformation, SID_AND_ATTRIBUTES, TOKEN_GROUPS, TOKEN_QUERY,
    TokenCapabilities, TokenIsAppContainer,
};
use windows_sys::Win32::System::Console::GetConsoleWindow;
use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

const MAX_TOKEN_CAPABILITIES_BYTES: usize = 16 * 1024;

pub(super) fn report(devices: CaptureDevices) -> Result<CaptureContainmentReport, String> {
    let token = current_token()?;
    let report = CaptureContainmentReport {
        app_container: is_app_container(&token)?,
        no_console_window: unsafe { GetConsoleWindow() }.is_null(),
        minimal_environment: has_minimal_environment(),
        camera_capability: false,
        microphone_capability: false,
    };
    let (camera_capability, microphone_capability) = token_capabilities(&token)?;
    let report = CaptureContainmentReport {
        camera_capability,
        microphone_capability,
        ..report
    };
    if !report.satisfies(devices) {
        return Err("capture child does not satisfy the granted containment contract".into());
    }
    Ok(report)
}

fn current_token() -> Result<OwnedHandle, String> {
    let mut handle = null_mut();
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut handle) } == 0 {
        return Err(format!(
            "open capture process token: {}",
            std::io::Error::last_os_error()
        ));
    }
    Ok(unsafe { OwnedHandle::from_raw_handle(handle as RawHandle) })
}

fn is_app_container(token: &OwnedHandle) -> Result<bool, String> {
    let mut value = 0_u32;
    let mut returned = 0_u32;
    let ok = unsafe {
        GetTokenInformation(
            token.as_raw_handle() as _,
            TokenIsAppContainer,
            (&mut value as *mut u32).cast(),
            size_of::<u32>() as u32,
            &mut returned,
        )
    };
    if ok == 0 || returned as usize != size_of::<u32>() {
        return Err(format!(
            "query capture AppContainer token: {}",
            std::io::Error::last_os_error()
        ));
    }
    Ok(value != 0)
}

fn token_capabilities(token: &OwnedHandle) -> Result<(bool, bool), String> {
    let mut bytes = 0_u32;
    unsafe {
        GetTokenInformation(
            token.as_raw_handle() as _,
            TokenCapabilities,
            null_mut(),
            0,
            &mut bytes,
        )
    };
    let minimum = offset_of!(TOKEN_GROUPS, Groups);
    if (bytes as usize) < minimum || bytes as usize > MAX_TOKEN_CAPABILITIES_BYTES {
        return Err("capture token capability list has an invalid size".into());
    }
    let mut storage = vec![0_usize; (bytes as usize).div_ceil(size_of::<usize>())];
    let capacity = storage.len() * size_of::<usize>();
    let mut returned = 0_u32;
    let ok = unsafe {
        GetTokenInformation(
            token.as_raw_handle() as _,
            TokenCapabilities,
            storage.as_mut_ptr().cast(),
            capacity as u32,
            &mut returned,
        )
    };
    if ok == 0 || returned as usize > capacity || (returned as usize) < minimum {
        return Err(format!(
            "query capture token capabilities: {}",
            std::io::Error::last_os_error()
        ));
    }
    let groups = unsafe { &*(storage.as_ptr().cast::<TOKEN_GROUPS>()) };
    let count = groups.GroupCount as usize;
    let extent = minimum
        .checked_add(
            count
                .checked_mul(size_of::<SID_AND_ATTRIBUTES>())
                .ok_or("capture capability count overflow")?,
        )
        .ok_or("capture capability extent overflow")?;
    if count > 2 || extent > returned as usize {
        return Err("capture token has unexpected capability SIDs".into());
    }
    let webcam = CapabilitySid::derive("webcam")?;
    let microphone = CapabilitySid::derive("microphone")?;
    let entries = unsafe {
        std::slice::from_raw_parts(
            (storage.as_ptr() as *const u8)
                .add(minimum)
                .cast::<SID_AND_ATTRIBUTES>(),
            count,
        )
    };
    let mut camera_present = false;
    let mut microphone_present = false;
    for entry in entries {
        if unsafe { EqualSid(entry.Sid, webcam.as_ptr()) } != 0 {
            if camera_present {
                return Err("duplicate webcam capability SID".into());
            }
            camera_present = true;
        } else if unsafe { EqualSid(entry.Sid, microphone.as_ptr()) } != 0 {
            if microphone_present {
                return Err("duplicate microphone capability SID".into());
            }
            microphone_present = true;
        } else {
            return Err("capture token has an unexpected capability SID".into());
        }
    }
    Ok((camera_present, microphone_present))
}

fn has_minimal_environment() -> bool {
    let mut system_root = false;
    for (name, _) in std::env::vars_os() {
        let name = name.to_string_lossy();
        if name.eq_ignore_ascii_case("SystemRoot") {
            system_root = true;
        }
        if !crate::renderer_process::renderer_environment_name_allowed(&name) {
            return false;
        }
    }
    system_root
}
