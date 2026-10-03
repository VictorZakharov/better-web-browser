//! Fixed GLES3 transform-feedback ABIs from the existing pinned ANGLE header.
type Varyings = unsafe extern "system" fn(u32, i32, *const *const i8, u32);
type Varying = unsafe extern "system" fn(u32, u32, i32, *mut i32, *mut i32, *mut u32, *mut i8);
type Action = unsafe extern "system" fn();
pub(super) struct Entries {
    pub gen_objects: super::extensions::GenArrays,
    pub bind: super::core_entries::BeginQuery,
    pub begin: super::core_entries::ReadBuffer,
    pub end: Action,
    pub pause: Action,
    pub resume: Action,
    pub varyings: Varyings,
    pub varying: Varying,
}
macro_rules! entry {
    ($name:literal, $kind:ty) => {{
        let pointer = unsafe { mozangle::egl::ffi::GetProcAddress($name.as_ptr()) };
        if pointer.is_null() {
            return Err(format!(
                "Required GLES3 transform entry unavailable: {}",
                $name.to_string_lossy()
            ));
        }
        // SAFETY: compile-time symbol and exact pinned Khronos ABI.
        unsafe {
            std::mem::transmute::<
                mozangle::egl::ffi::types::__eglMustCastToProperFunctionPointerType,
                $kind,
            >(pointer)
        }
    }};
}
impl Entries {
    pub(super) fn load() -> Result<Self, String> {
        let _: super::extensions::DeleteArrays = entry!(
            c"glDeleteTransformFeedbacks",
            super::extensions::DeleteArrays
        );
        Ok(Self {
            gen_objects: entry!(c"glGenTransformFeedbacks", super::extensions::GenArrays),
            bind: entry!(c"glBindTransformFeedback", super::core_entries::BeginQuery),
            begin: entry!(c"glBeginTransformFeedback", super::core_entries::ReadBuffer),
            end: entry!(c"glEndTransformFeedback", Action),
            pause: entry!(c"glPauseTransformFeedback", Action),
            resume: entry!(c"glResumeTransformFeedback", Action),
            varyings: entry!(c"glTransformFeedbackVaryings", Varyings),
            varying: entry!(c"glGetTransformFeedbackVarying", Varying),
        })
    }
}
pub(super) unsafe fn delete_native(name: u32) {
    let pointer =
        unsafe { mozangle::egl::ffi::GetProcAddress(c"glDeleteTransformFeedbacks".as_ptr()) };
    // Construction verified this fixed symbol. Destruction runs on the current owner.
    let function = unsafe {
        std::mem::transmute::<
            mozangle::egl::ffi::types::__eglMustCastToProperFunctionPointerType,
            super::extensions::DeleteArrays,
        >(pointer)
    };
    unsafe { function(1, &name) };
}
