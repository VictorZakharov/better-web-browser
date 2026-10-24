use super::*;

impl NativeContext {
    pub(crate) fn shared_copy_context(&self) -> Result<Self, String> {
        // EGL shares texture storage; bindings, queries and transform feedback
        // remain in the author's context. EGL 1.5 section 3.7.1 defines sharing.
        let mut copy = Self {
            display: self.display,
            context: ptr::null(),
            surface: ptr::null(),
            config: self.config,
            _thread: PhantomData,
        };
        let surface = [
            egl::WIDTH as i32,
            1,
            egl::HEIGHT as i32,
            1,
            egl::NONE as i32,
        ];
        copy.surface =
            unsafe { egl::CreatePbufferSurface(self.display, self.config, surface.as_ptr()) };
        if copy.surface.is_null() {
            return Err(error("create private GPU copy pbuffer"));
        }
        let attributes = [
            egl::CONTEXT_CLIENT_VERSION as i32,
            3,
            0x30fb,
            super::super::ApiVersion::Two.provider_minor(),
            0x3483,
            0,
            0x33ac,
            1,
            0x3453,
            1,
            0x3452,
            0,
            egl::NONE as i32,
        ];
        copy.context = unsafe {
            egl::CreateContext(self.display, self.config, self.context, attributes.as_ptr())
        };
        if copy.context.is_null() {
            return Err(error("create private shared GPU copy context"));
        }
        Ok(copy)
    }
}
