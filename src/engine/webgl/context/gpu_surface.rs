//! Thread-affine ANGLE presentation surfaces; the legacy DXGI handle is not a kernel handle.
use super::{NativeContext, egl, error, ptr};
use std::ffi::{CStr, c_void};
use windows::Win32::Foundation::HANDLE;
use windows::Win32::Graphics::Direct3D11::ID3D11Device;
use windows::Win32::Graphics::Dxgi::IDXGIKeyedMutex;
use windows::core::Interface;

type QuerySurface = unsafe extern "system" fn(
    egl::types::EGLDisplay,
    egl::types::EGLSurface,
    i32,
    *mut *mut c_void,
) -> u32;
type QueryDisplay = unsafe extern "system" fn(egl::types::EGLDisplay, i32, *mut isize) -> u32;
type QueryDevice = unsafe extern "system" fn(*mut c_void, i32, *mut isize) -> u32;

pub(in crate::engine::webgl) struct SharedSurface {
    display: egl::types::EGLDisplay,
    surface: egl::types::EGLSurface,
    handle: HANDLE,
    pub(in crate::engine::webgl) mutex: IDXGIKeyedMutex,
}

impl NativeContext {
    pub(in crate::engine::webgl) fn d3d_device(&self) -> Result<ID3D11Device, String> {
        let display = mozangle::egl::get_proc_address("eglQueryDisplayAttribEXT");
        let device = mozangle::egl::get_proc_address("eglQueryDeviceAttribEXT");
        if display.is_null() || device.is_null() {
            return Err("ANGLE D3D device query unavailable".into());
        }
        let query_display: QueryDisplay = unsafe { std::mem::transmute(display) };
        let query_device: QueryDevice = unsafe { std::mem::transmute(device) };
        let mut native_device = 0;
        let mut d3d = 0;
        if unsafe { query_display(self.display, 0x322c, &mut native_device) } == 0
            || unsafe { query_device(native_device as *mut c_void, 0x33a1, &mut d3d) } == 0
            || d3d == 0
        {
            return Err(error("query ANGLE D3D11 device"));
        }
        // The EGL display owns the returned interface. Clone acquires our independent COM reference.
        let d3d = d3d as *mut c_void;
        unsafe { ID3D11Device::from_raw_borrowed(&d3d) }
            .cloned()
            .ok_or_else(|| "ANGLE returned no D3D11 device".into())
    }

    pub(in crate::engine::webgl) fn shared_surface(
        &self,
        width: u32,
        height: u32,
    ) -> Result<SharedSurface, String> {
        let extensions = unsafe { egl::QueryString(self.display, egl::EXTENSIONS as i32) };
        if extensions.is_null() {
            return Err(error("query ANGLE surface extensions"));
        }
        let extensions = unsafe { CStr::from_ptr(extensions) }.to_bytes();
        for required in [
            b"EGL_ANGLE_surface_d3d_texture_2d_share_handle".as_slice(),
            b"EGL_ANGLE_keyed_mutex",
        ] {
            if !extensions
                .split(u8::is_ascii_whitespace)
                .any(|name| name == required)
            {
                return Err("ANGLE shared presentation surfaces unavailable".into());
            }
        }
        let query = mozangle::egl::get_proc_address("eglQuerySurfacePointerANGLE");
        if query.is_null() {
            return Err("ANGLE surface pointer query unavailable".into());
        }
        let query: QuerySurface = unsafe { std::mem::transmute(query) };
        let attributes = [
            egl::WIDTH as i32,
            width as i32,
            egl::HEIGHT as i32,
            height as i32,
            egl::NONE as i32,
        ];
        let surface =
            unsafe { egl::CreatePbufferSurface(self.display, self.config, attributes.as_ptr()) };
        if surface.is_null() {
            return Err(error("create shared ANGLE presentation surface"));
        }
        let mut handle = ptr::null_mut();
        let mut mutex = ptr::null_mut();
        // Making the surface current admits its real D3D storage, rather than reporting a lazy NULL handle.
        let success = unsafe {
            egl::MakeCurrent(self.display, surface, surface, self.context) != 0
                && query(self.display, surface, 0x3200, &mut handle) != 0
                && query(self.display, surface, 0x33a2, &mut mutex) != 0
        };
        let restored = self.make_current();
        let owned_mutex = unsafe { IDXGIKeyedMutex::from_raw_borrowed(&mutex) }.cloned();
        if !success || handle.is_null() || owned_mutex.is_none() || restored.is_err() {
            unsafe {
                egl::DestroySurface(self.display, surface);
            }
            return Err(error("admit keyed ANGLE shared presentation surface"));
        }
        Ok(SharedSurface {
            display: self.display,
            surface,
            handle: HANDLE(handle),
            mutex: owned_mutex.unwrap(),
        })
    }

    pub(in crate::engine::webgl) fn make_surface_current(
        &self,
        surface: &SharedSurface,
    ) -> Result<(), String> {
        if surface.display != self.display {
            return Err("presentation surface belongs to another ANGLE display".into());
        }
        if unsafe { egl::MakeCurrent(self.display, surface.surface, surface.surface, self.context) }
            == 0
        {
            return Err(error("activate ANGLE presentation surface"));
        }
        Ok(())
    }
}

impl SharedSurface {
    pub(in crate::engine::webgl) fn handle(&self) -> HANDLE {
        self.handle
    }
}

impl Drop for SharedSurface {
    fn drop(&mut self) {
        // EGL owns the texture and its legacy share handle; CloseHandle would be incorrect.
        unsafe {
            egl::DestroySurface(self.display, self.surface);
        }
    }
}

#[cfg(test)]
mod tests;
