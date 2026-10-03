//! EGL pbuffer lifecycle. No HWND, client pointers, or uninitialized author-visible resources.
use mozangle::egl::ffi as egl;
use std::{marker::PhantomData, ptr, rc::Rc};

pub(super) struct NativeContext {
    display: egl::types::EGLDisplay,
    context: egl::types::EGLContext,
    surface: egl::types::EGLSurface,
    // ANGLE contexts are thread-affine; never make this object Send/Sync.
    _thread: PhantomData<Rc<()>>,
}

impl NativeContext {
    pub(super) fn new() -> Result<Self, String> {
        // WARP is an actual D3D11 software renderer and works on headless CI. Do not
        // use ANGLE's NULL backend (which validates commands but cannot produce pixels).
        // https://github.com/google/angle/blob/main/extensions/EGL_ANGLE_platform_angle.txt
        let attributes = [0x3203, 0x3208, 0x3209, 0x320B, egl::NONE as i32];
        // SAFETY: terminated integer attribute array; no native window/display pointer.
        let display =
            unsafe { egl::GetPlatformDisplayEXT(0x3202, ptr::null_mut(), attributes.as_ptr()) };
        if display.is_null() {
            return Err(error("create ANGLE display"));
        }
        let mut result = Self {
            display,
            context: ptr::null(),
            surface: ptr::null(),
            _thread: PhantomData,
        };
        let mut major = 0;
        let mut minor = 0;
        unsafe {
            if egl::Initialize(display, &mut major, &mut minor) == 0 {
                return Err(error("initialize ANGLE"));
            }
            if egl::BindAPI(egl::OPENGL_ES_API) == 0 {
                return Err(error("bind OpenGL ES API"));
            }
            let attributes = [
                egl::SURFACE_TYPE as i32,
                egl::PBUFFER_BIT as i32,
                egl::RENDERABLE_TYPE as i32,
                0x0004, // EGL_OPENGL_ES2_BIT, matches the public WebGL1 shader contract.
                egl::RED_SIZE as i32,
                8,
                egl::GREEN_SIZE as i32,
                8,
                egl::BLUE_SIZE as i32,
                8,
                egl::ALPHA_SIZE as i32,
                8,
                egl::NONE as i32,
            ];
            let mut config = ptr::null();
            let mut count = 0;
            if egl::ChooseConfig(display, attributes.as_ptr(), &mut config, 1, &mut count) == 0
                || count != 1
            {
                return Err(error("choose GLES2 pbuffer format"));
            }
            let attributes = [
                egl::WIDTH as i32,
                1,
                egl::HEIGHT as i32,
                1,
                egl::NONE as i32,
            ];
            result.surface = egl::CreatePbufferSurface(display, config, attributes.as_ptr());
            if result.surface.is_null() {
                return Err(error("create GLES2 pbuffer"));
            }
            // WebGL shader restrictions, zero-initialized resources, and buffer-only attributes.
            let attributes = [
                egl::CONTEXT_CLIENT_VERSION as i32,
                2,
                // Match WebGL1's native shader rules, including EXT_draw_buffers.
                // A silently upgraded GLES3 compatibility context instead uses
                // WebGL2's one-element gl_FragData rule for ESSL100.
                // EGL_ANGLE_create_context_backwards_compatible.
                0x3483,
                0,
                0x33AC,
                1,
                0x3453,
                1,
                0x3452,
                0,
                egl::NONE as i32,
            ];
            result.context = egl::CreateContext(display, config, ptr::null(), attributes.as_ptr());
            if result.context.is_null() {
                return Err(error("create WebGL-compatible GLES2 backend"));
            }
        }
        result.make_current()?;
        super::extensions::initialize_storage()?;
        Ok(result)
    }
    pub(super) fn make_current(&self) -> Result<(), String> {
        // SAFETY: all handles belong to this live, thread-affine owner.
        if unsafe { egl::MakeCurrent(self.display, self.surface, self.surface, self.context) } == 0
        {
            Err(error("make WebGL context current"))
        } else {
            Ok(())
        }
    }
}
impl Drop for NativeContext {
    fn drop(&mut self) {
        self.destroy();
    }
}
impl NativeContext {
    // The native owner thread performs this, including partial-construction cleanup.
    pub(super) fn destroy(&mut self) {
        if self.context.is_null() && self.surface.is_null() {
            return;
        }
        // EGL displays are shared by ANGLE. Terminate is deliberately not called per context:
        // it invalidates peer contexts on the same display. Process teardown releases the display.
        // Each context and pbuffer, including partially initialized ones, is still destroyed.
        unsafe {
            if egl::GetCurrentContext() == self.context {
                egl::MakeCurrent(self.display, ptr::null(), ptr::null(), ptr::null());
            }
            if !self.context.is_null() {
                egl::DestroyContext(self.display, self.context);
            }
            if !self.surface.is_null() {
                egl::DestroySurface(self.display, self.surface);
            }
        }
        self.context = ptr::null();
        self.surface = ptr::null();
    }
}
fn error(action: &str) -> String {
    format!("{action}: EGL error {:#x}", unsafe { egl::GetError() })
}

#[cfg(test)]
mod tests {
    use super::*;
    use mozangle::gles::ffi as gl;
    #[test]
    fn webgl_exact_native_version_allocates_private_default_surface() {
        super::super::session::run_native_test(|| {
            let _context = super::super::WebGl::new(8, 4, super::super::Options::default())
                .expect("exact native WebGL provider and private framebuffer");
        });
    }
    #[test]
    fn webgl_native_context_enables_compiler_restrictions() {
        super::super::session::run_native_test(|| {
            let _context = NativeContext::new().unwrap();
            let version = unsafe { std::ffi::CStr::from_ptr(gl::GetString(gl::VERSION).cast()) }
                .to_str()
                .unwrap();
            assert!(
                version.starts_with("OpenGL ES 2.0"),
                "requested exact GLES2 backend but received {version}"
            );
            let pointer = unsafe { gl::GetString(gl::EXTENSIONS) };
            assert!(!pointer.is_null());
            let extensions = unsafe { std::ffi::CStr::from_ptr(pointer.cast()) }
                .to_str()
                .unwrap();
            assert!(
                extensions
                    .split_ascii_whitespace()
                    .any(|value| value == "GL_ANGLE_webgl_compatibility"),
                "native WebGL compatibility is not enabled"
            );
        });
    }
}
