//! Private GLES fences establish completed bytes before a keyed-mutex handoff.
use super::super::{context::NativeContext, gl, sync_entries};

pub(super) struct Completion {
    pub(super) handle: sync_entries::Handle,
    pub(super) key: u64,
    pub(super) sequence: u64,
}

impl super::copy::Renderer {
    pub(super) fn order_copy_after_author(&self, author: &NativeContext) -> Result<(), String> {
        author.make_current()?;
        let fence = unsafe { (self.sync.fence)(0x9117, 0) };
        unsafe { gl::Flush() };
        if fence.is_null() {
            return Err("private GPU source fence allocation failed".into());
        }
        let current = self.make_current();
        if current.is_ok() {
            unsafe {
                (self.sync.server_wait)(fence, 0, u64::MAX);
                (self.sync.delete)(fence);
            }
        } else {
            unsafe { (self.sync.delete)(fence) };
        }
        current
    }

    pub(super) fn poll(&self, completion: &Completion) -> Result<bool, String> {
        self.make_current()?;
        // Zero timeout never stalls the native owner. Only signaled fences may
        // publish bytes to D3D; flushing by itself does not prove completion.
        // https://registry.khronos.org/OpenGL-Refpages/es3/html/glClientWaitSync.xhtml
        match unsafe { (self.sync.client_wait)(completion.handle, 0, 0) } {
            0x911a | 0x911c => Ok(true),
            0x911b => Ok(false),
            status => Err(format!("private GPU frame fence failed: GL {status:#x}")),
        }
    }

    pub(super) fn delete_completion(&self, completion: Completion) {
        unsafe { (self.sync.delete)(completion.handle) };
    }

    pub(super) fn finish_pending(&self) -> Result<(), String> {
        self.make_current()?;
        // Synchronous repair/retirement is exceptional. Ordinary publication
        // polls fences and never calls Finish.
        unsafe { gl::Finish() };
        Ok(())
    }

    pub(super) fn order_author_after_copy(
        &self,
        author: &NativeContext,
        fence: sync_entries::Handle,
    ) -> Result<(), String> {
        author.make_current()?;
        // The shared source texture may be cleared or redrawn immediately.
        // Order those writes on the GPU after the private copy, without waiting
        // on the CPU or disturbing author framebuffer/query/feedback state.
        unsafe {
            (self.sync.server_wait)(fence, 0, u64::MAX);
            gl::Flush();
        }
        Ok(())
    }
}
