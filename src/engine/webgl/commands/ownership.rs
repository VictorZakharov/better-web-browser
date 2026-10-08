//! Compiler ordering and storage retirement also apply to owned native uploads.
use super::*;

impl WebGl {
    pub(in crate::engine::webgl) fn dispatch(
        &mut self,
        c: &Command,
        bytes: Option<&[u8]>,
    ) -> Result<Value> {
        self.dispatch_data(c, bytes.map(std::borrow::Cow::Borrowed))
    }

    pub(in crate::engine::webgl) fn dispatch_data(
        &mut self,
        c: &Command,
        bytes: Option<std::borrow::Cow<'_, [u8]>>,
    ) -> Result<Value> {
        self.compiler_barrier(c)?;
        let result = if c.op == "bufferData" {
            self.buffer_data(c, bytes)
        } else {
            self.dispatch_inner(c, bytes.as_deref())
        };
        self.reclaim_retired_storage()?;
        result
    }
}
