//! Fixed OES_draw_buffers_indexed ABI from the pinned ANGLE gl2ext.h.
use mozangle::egl::ffi as egl;

type Toggle = unsafe extern "system" fn(u32, u32);
type Equation = unsafe extern "system" fn(u32, u32);
type EquationSeparate = unsafe extern "system" fn(u32, u32, u32);
type Function = unsafe extern "system" fn(u32, u32, u32);
type FunctionSeparate = unsafe extern "system" fn(u32, u32, u32, u32, u32);
type Mask = unsafe extern "system" fn(u32, u8, u8, u8, u8);
type Integer = unsafe extern "system" fn(u32, u32, *mut i32);
type Boolean = unsafe extern "system" fn(u32, u32, *mut u8);

#[derive(Clone, Copy)]
pub(super) struct Entries {
    pub enable: Toggle,
    pub disable: Toggle,
    pub equation: Equation,
    pub equation_separate: EquationSeparate,
    pub function: Function,
    pub function_separate: FunctionSeparate,
    pub mask: Mask,
    pub integer: Integer,
    pub boolean: Boolean,
}

macro_rules! entry {
    ($name:literal, $kind:ty) => {{
        // SAFETY: fixed provider-owned symbols, exact Khronos signatures. No
        // author name, function pointer or pointer-sized offset is accepted.
        let pointer = unsafe { egl::GetProcAddress($name.as_ptr()) };
        if pointer.is_null() {
            return None;
        }
        unsafe {
            std::mem::transmute::<egl::types::__eglMustCastToProperFunctionPointerType, $kind>(
                pointer,
            )
        }
    }};
}

impl Entries {
    pub fn load() -> Option<Self> {
        Some(Self {
            enable: entry!(c"glEnableiOES", Toggle),
            disable: entry!(c"glDisableiOES", Toggle),
            equation: entry!(c"glBlendEquationiOES", Equation),
            equation_separate: entry!(c"glBlendEquationSeparateiOES", EquationSeparate),
            function: entry!(c"glBlendFunciOES", Function),
            function_separate: entry!(c"glBlendFuncSeparateiOES", FunctionSeparate),
            mask: entry!(c"glColorMaskiOES", Mask),
            integer: entry!(c"glGetIntegeri_v", Integer),
            boolean: entry!(c"glGetBooleani_v", Boolean),
        })
    }
}
