//! Location queries have separate string, program-state and reserved-name rules.
//! WebGL §Characters Outside the GLSL Source Character Set; GLES2 §2.10.4.
use super::{ApiVersion, Result, gl};
use std::ffi::CString;

pub(super) fn validate(value: &str, budget: usize) -> Result<()> {
    // These entry points accept the GLSL source character set, not every ASCII
    // byte and not only identifier grammar (array/structure lookups need []/.).
    if value.len() > budget
        || !value.bytes().all(|byte| {
            matches!(byte, 9..=13)
                || matches!(byte, 32..=126)
                    && !matches!(byte, b'"' | b'\'' | b'$' | b'@' | b'\\' | b'`')
        })
    {
        return Err(gl::INVALID_VALUE);
    }
    Ok(())
}

pub(super) fn location(value: &str, api: ApiVersion) -> Result<CString> {
    validate(value, api.query_name_budget())?;
    CString::new(value).map_err(|_| gl::INVALID_VALUE)
}

pub(super) fn reserved(value: &str) -> bool {
    ["gl_", "webgl_", "_webgl_"]
        .iter()
        .any(|prefix| value.starts_with(prefix))
}

pub(super) fn linked(program: u32) -> Result<()> {
    let mut linked = 0;
    // The caller has already validated the program's private native name.
    unsafe {
        gl::GetProgramiv(program, gl::LINK_STATUS, &mut linked);
    }
    if linked == 0 {
        Err(gl::INVALID_OPERATION)
    } else {
        Ok(())
    }
}
