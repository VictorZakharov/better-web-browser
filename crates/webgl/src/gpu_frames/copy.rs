//! Private shared-context copies preserve author GL state and query results.
use super::super::{context::NativeContext, core_entries::CoreEntries};
use super::*;
use std::ffi::CStr;

const VERTEX: &CStr = cr"#version 300 es
void main() {
    vec2 p = vec2(float((gl_VertexID << 1) & 2), float(gl_VertexID & 2));
    gl_Position = vec4(p * 2.0 - 1.0, 0.0, 1.0);
}";
const FRAGMENT: &CStr = cr"#version 300 es
precision highp float;
precision highp int;
uniform sampler2D frame;
uniform int mode;
uniform int height;
out vec4 color;
void main() {
    ivec2 p = ivec2(int(gl_FragCoord.x), height - 1 - int(gl_FragCoord.y));
    uvec4 v = uvec4(floor(texelFetch(frame, p, 0) * 255.0 + 0.5));
    if (mode == 0) v.a = 255u;
    else if (mode == 1) v.rgb = min(v.rgb, uvec3(v.a));
    else v.rgb = (v.rgb * v.a + uvec3(127u)) / 255u;
    color = vec4(v) / 255.0;
}";

pub(super) struct Renderer {
    native: NativeContext,
    program: u32,
    vertex: u32,
    fragment: u32,
    mode: i32,
    height: i32,
    read_framebuffer: u32,
    resolve_framebuffer: u32,
    blit: super::super::core_entries::Blit,
    read_buffer: super::super::core_entries::ReadBuffer,
    pub(super) sync: super::super::sync_entries::Entries,
}
impl Renderer {
    pub(super) fn new(author: &NativeContext) -> Result<Self, String> {
        let native = author.shared_copy_context()?;
        let core = CoreEntries::load()?;
        let mut renderer = Self {
            native,
            program: 0,
            vertex: 0,
            fragment: 0,
            mode: -1,
            height: -1,
            read_framebuffer: 0,
            resolve_framebuffer: 0,
            blit: core.blit,
            read_buffer: core.read_buffer,
            sync: core.sync,
        };
        let result = (|| {
            renderer.native.make_current()?;
            // Conversion produces exact RGBA8 bytes, independent of author dithering.
            unsafe {
                gl::Disable(gl::DITHER);
            }
            unsafe {
                gl::GenFramebuffers(1, &mut renderer.read_framebuffer);
                gl::GenFramebuffers(1, &mut renderer.resolve_framebuffer);
            }
            renderer.vertex = shader(gl::VERTEX_SHADER, VERTEX)?;
            renderer.fragment = shader(gl::FRAGMENT_SHADER, FRAGMENT)?;
            unsafe {
                renderer.program = gl::CreateProgram();
                gl::AttachShader(renderer.program, renderer.vertex);
                gl::AttachShader(renderer.program, renderer.fragment);
                gl::LinkProgram(renderer.program);
                let mut status = 0;
                gl::GetProgramiv(renderer.program, gl::LINK_STATUS, &mut status);
                if status == 0 {
                    return Err("private GPU frame program failed to link".into());
                }
                renderer.mode = gl::GetUniformLocation(renderer.program, c"mode".as_ptr());
                renderer.height = gl::GetUniformLocation(renderer.program, c"height".as_ptr());
                if renderer.mode < 0 || renderer.height < 0 {
                    return Err("private GPU frame uniforms unavailable".into());
                }
            }
            CoreEntries::load()?;
            Ok::<_, String>(())
        })();
        let restored = author.make_current();
        result?;
        restored?;
        Ok(renderer)
    }
    pub(super) fn make_current(&self) -> Result<(), String> {
        self.native.make_current()
    }
    pub(super) fn detach_sources(&self) -> Result<(), String> {
        self.native.make_current()?;
        unsafe {
            gl::BindTexture(gl::TEXTURE_2D, 0);
            gl::BindFramebuffer(0x8ca8, self.read_framebuffer);
            gl::FramebufferRenderbuffer(0x8ca8, gl::COLOR_ATTACHMENT0, gl::RENDERBUFFER, 0);
            gl::BindFramebuffer(0x8ca9, self.resolve_framebuffer);
            gl::FramebufferTexture2D(0x8ca9, gl::COLOR_ATTACHMENT0, gl::TEXTURE_2D, 0, 0);
            gl::BindFramebuffer(gl::FRAMEBUFFER, 0);
            let error = gl::GetError();
            if error != gl::NO_ERROR {
                return Err(format!(
                    "private GPU source retirement failed: GL {error:#x}"
                ));
            }
        }
        Ok(())
    }
    pub(super) fn resolve(
        &self,
        renderbuffer: u32,
        texture: u32,
        width: u32,
        height: u32,
    ) -> Result<(), String> {
        self.native.make_current()?;
        unsafe {
            gl::BindFramebuffer(0x8ca8, self.read_framebuffer);
            gl::FramebufferRenderbuffer(
                0x8ca8,
                gl::COLOR_ATTACHMENT0,
                gl::RENDERBUFFER,
                renderbuffer,
            );
            (self.read_buffer)(gl::COLOR_ATTACHMENT0);
            gl::BindFramebuffer(0x8ca9, self.resolve_framebuffer);
            gl::FramebufferTexture2D(0x8ca9, gl::COLOR_ATTACHMENT0, gl::TEXTURE_2D, texture, 0);
            if gl::CheckFramebufferStatus(0x8ca8) != gl::FRAMEBUFFER_COMPLETE
                || gl::CheckFramebufferStatus(0x8ca9) != gl::FRAMEBUFFER_COMPLETE
            {
                return Err("private GPU resolve framebuffer is incomplete".into());
            }
            (self.blit)(
                0,
                0,
                width as i32,
                height as i32,
                0,
                0,
                width as i32,
                height as i32,
                gl::COLOR_BUFFER_BIT,
                gl::NEAREST,
            );
            let error = gl::GetError();
            gl::Flush();
            if error != gl::NO_ERROR {
                return Err(format!("private GPU resolve failed: GL {error:#x}"));
            }
        }
        Ok(())
    }
    pub(super) fn copy(
        &self,
        surface: &SharedSurface,
        texture: u32,
        width: u32,
        height: u32,
        alpha: bool,
        premultiplied: bool,
    ) -> Result<super::super::sync_entries::Handle, String> {
        self.native.make_surface_current(surface)?;
        unsafe {
            gl::BindFramebuffer(gl::FRAMEBUFFER, 0);
            gl::Viewport(0, 0, width as i32, height as i32);
            gl::UseProgram(self.program);
            gl::ActiveTexture(gl::TEXTURE0);
            gl::BindTexture(gl::TEXTURE_2D, texture);
            gl::Uniform1i(
                self.mode,
                if !alpha {
                    0
                } else if premultiplied {
                    1
                } else {
                    2
                },
            );
            gl::Uniform1i(self.height, height as i32);
            gl::DrawArrays(gl::TRIANGLES, 0, 3);
            let error = gl::GetError();
            if error != gl::NO_ERROR {
                return Err(format!("private GPU frame copy failed: GL {error:#x}"));
            }
            let fence = (self.sync.fence)(0x9117, 0);
            gl::Flush();
            if fence.is_null() {
                return Err("private GPU frame fence allocation failed".into());
            }
            Ok(fence)
        }
    }
}
fn shader(kind: u32, source: &CStr) -> Result<u32, String> {
    let shader = unsafe { gl::CreateShader(kind) };
    if shader == 0 {
        return Err("private GPU frame shader allocation failed".into());
    }
    unsafe {
        gl::ShaderSource(shader, 1, &source.as_ptr(), std::ptr::null());
        gl::CompileShader(shader);
    }
    let mut status = 0;
    unsafe {
        gl::GetShaderiv(shader, gl::COMPILE_STATUS, &mut status);
    }
    if status == 0 {
        let mut log = vec![0_u8; 4096];
        let mut length = 0;
        unsafe {
            gl::GetShaderInfoLog(
                shader,
                log.len() as i32,
                &mut length,
                log.as_mut_ptr().cast(),
            );
            gl::DeleteShader(shader);
        }
        return Err(format!(
            "private GPU frame shader failed: {}",
            String::from_utf8_lossy(&log[..(length.max(0) as usize).min(log.len())])
        ));
    }
    Ok(shader)
}
impl Drop for Renderer {
    fn drop(&mut self) {
        if self.native.make_current().is_ok() {
            unsafe {
                gl::DeleteProgram(self.program);
                gl::DeleteShader(self.vertex);
                gl::DeleteShader(self.fragment);
                gl::DeleteFramebuffers(1, &self.read_framebuffer);
                gl::DeleteFramebuffers(1, &self.resolve_framebuffer);
            }
        }
    }
}
