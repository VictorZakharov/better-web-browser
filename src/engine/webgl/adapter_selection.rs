//! Windows adapter ordering uses the OS implementation, not vendor-name guesses.
//! windows 0.62.2 is an existing MIT/Apache-2.0 dependency; only DXGI bindings
//! are newly enabled. No native window, driver DLL download or borrowed browser.
use super::creation_options::PowerPreference;
use mozangle::egl::ffi as egl;
use windows::Win32::Graphics::Dxgi::{
    CreateDXGIFactory1, DXGI_ADAPTER_FLAG_SOFTWARE, DXGI_GPU_PREFERENCE_HIGH_PERFORMANCE,
    DXGI_GPU_PREFERENCE_MINIMUM_POWER, IDXGIAdapter1, IDXGIFactory6,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct AdapterId {
    pub high: i32,
    pub low: u32,
}

pub(super) fn preferred(preference: PowerPreference) -> Option<AdapterId> {
    if preference == PowerPreference::Default || !supports_luid() {
        return None;
    }
    // A hint must not prevent rendering on systems without DXGI 1.6 or where
    // sandbox/device policy denies enumeration. Ordinary hardware selection
    // follows independently; the hint never enables a software adapter.
    // https://learn.microsoft.com/windows/win32/api/dxgi1_6/nf-dxgi1_6-idxgifactory6-enumadapterbygpupreference
    let factory: IDXGIFactory6 = unsafe { CreateDXGIFactory1() }.ok()?;
    let preference = if preference == PowerPreference::LowPower {
        DXGI_GPU_PREFERENCE_MINIMUM_POWER
    } else {
        DXGI_GPU_PREFERENCE_HIGH_PERFORMANCE
    };
    // Enumeration is bounded even if a malformed provider never reports EOF.
    for index in 0..16 {
        let adapter: IDXGIAdapter1 =
            unsafe { factory.EnumAdapterByGpuPreference(index, preference) }.ok()?;
        let description = unsafe { adapter.GetDesc1() }.ok()?;
        if description.Flags & DXGI_ADAPTER_FLAG_SOFTWARE.0 as u32 != 0 {
            continue;
        }
        let id = AdapterId {
            high: description.AdapterLuid.HighPart,
            low: description.AdapterLuid.LowPart,
        };
        if id.high != 0 || id.low != 0 {
            return Some(id);
        }
    }
    None
}

fn supports_luid() -> bool {
    // EGL client extension query, before any display exists. Fixed provider
    // string; author input cannot select an entry point or platform attribute.
    let pointer = unsafe { egl::QueryString(std::ptr::null(), egl::EXTENSIONS as i32) };
    if pointer.is_null() {
        return false;
    }
    unsafe { std::ffi::CStr::from_ptr(pointer) }
        .to_bytes()
        .split(u8::is_ascii_whitespace)
        .any(|name| name == b"EGL_ANGLE_platform_angle_d3d_luid")
}
