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
    #[cfg(test)]
    pub(super) fn new() -> Result<Self, String> {
        Self::for_backend(
            super::ApiVersion::One,
            super::backend_policy::Backend::Software,
        )
    }

    pub(super) fn for_backend(
        api: super::ApiVersion,
        backend: super::backend_policy::Backend,
    ) -> Result<Self, String> {
        let attributes = backend.attributes();
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
                api.renderable_bit(),
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
                return Err(error("choose versioned GLES pbuffer format"));
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
                return Err(error("create versioned GLES pbuffer"));
            }
            // WebGL shader restrictions, zero-initialized resources, and buffer-only attributes.
            let attributes = [
                egl::CONTEXT_CLIENT_VERSION as i32,
                api.client_version(),
                0x30fb, // EGL_CONTEXT_MINOR_VERSION_KHR, trusted provider selection only.
                api.provider_minor(),
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
                return Err(error("create versioned WebGL-compatible GLES backend"));
            }
        }
        result.make_current()?;
        if api == super::ApiVersion::One {
            super::extensions::initialize_storage()?;
        }
        Ok(result)
    }
    pub(super) fn make_current(&self) -> Result<(), String> {
        // EGL's thread-local getters are authoritative even after a peer's
        // creation or destruction. Do not use a browser-side cached identity:
        // teardown can change the current binding outside the command path.
        if unsafe {
            egl::GetCurrentContext() == self.context
                && egl::GetCurrentDisplay() == self.display
                && egl::GetCurrentSurface(egl::DRAW as i32) == self.surface
                && egl::GetCurrentSurface(egl::READ as i32) == self.surface
        } {
            return Ok(());
        }
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
    fn repeated_activation_observes_native_peer_binding_and_unbinding() {
        super::super::session::run_native_test(|| {
            let first = NativeContext::new().unwrap();
            let second = NativeContext::new().unwrap();
            assert_eq!(unsafe { egl::GetCurrentContext() }, second.context);
            first.make_current().unwrap();
            for _ in 0..32 {
                first.make_current().unwrap();
            }
            assert_eq!(unsafe { egl::GetCurrentContext() }, first.context);
            assert_eq!(unsafe { egl::GetCurrentDisplay() }, first.display);
            assert_eq!(
                unsafe { egl::GetCurrentSurface(egl::DRAW as i32) },
                first.surface
            );
            assert_eq!(
                unsafe { egl::GetCurrentSurface(egl::READ as i32) },
                first.surface
            );
            // Native unbinding is not visible in a browser-side identity cache.
            assert_ne!(
                unsafe { egl::MakeCurrent(first.display, ptr::null(), ptr::null(), ptr::null()) },
                0
            );
            first.make_current().unwrap();
            assert_eq!(unsafe { egl::GetCurrentContext() }, first.context);
            second.make_current().unwrap();
            drop(second);
            first.make_current().unwrap();
            assert_eq!(unsafe { egl::GetCurrentContext() }, first.context);
        });
    }
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
