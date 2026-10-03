//! GLsync is pointer-sized; never route it through the GLuint object adapter.
use std::ffi::c_void;
pub(super) type Handle = *const c_void;
type Fence = unsafe extern "system" fn(u32, u32) -> Handle;
type Delete = unsafe extern "system" fn(Handle);
type ClientWait = unsafe extern "system" fn(Handle, u32, u64) -> u32;
type ServerWait = unsafe extern "system" fn(Handle, u32, u64);
type Query = unsafe extern "system" fn(Handle, u32, i32, *mut i32, *mut i32);

pub(super) struct Entries {
    pub fence: Fence,
    pub delete: Delete,
    pub client_wait: ClientWait,
    pub server_wait: ServerWait,
    pub query: Query,
}
macro_rules! entry {
    ($name:literal, $kind:ty) => {{
        // SAFETY: a fixed pinned GLES3 ABI, resolved only from linked ANGLE.
        let pointer = unsafe { mozangle::egl::ffi::GetProcAddress($name.as_ptr()) };
        if pointer.is_null() {
            return Err(format!(
                "Required GLES3 sync entry unavailable: {}",
                $name.to_string_lossy()
            ));
        }
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
        Ok(Self {
            fence: entry!(c"glFenceSync", Fence),
            delete: entry!(c"glDeleteSync", Delete),
            client_wait: entry!(c"glClientWaitSync", ClientWait),
            server_wait: entry!(c"glWaitSync", ServerWait),
            query: entry!(c"glGetSynciv", Query),
        })
    }
}
