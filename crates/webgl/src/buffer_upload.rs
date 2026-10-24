//! The independent bridge allocation becomes the validated CPU mirror after native success.
use super::{Command, Kind, MAX_UPLOAD_BYTES, Result, WebGl, gl};
use serde_json::Value;
use std::borrow::Cow;

impl WebGl {
    pub(super) fn buffer_data(
        &mut self,
        c: &Command,
        bytes: Option<Cow<'_, [u8]>>,
    ) -> Result<Value> {
        let target = c.u(0)?;
        let id = self.bound_buffer(target)?;
        self.validate_transform_buffer_use(target, id)?;
        let size = c.u(1)? as usize;
        let usage = c.u(2)?;
        let core_usage = self.options.api == super::ApiVersion::Two
            && [0x88e1, 0x88e2, 0x88e5, 0x88e6, 0x88e9, 0x88ea].contains(&usage);
        if ![gl::STATIC_DRAW, gl::DYNAMIC_DRAW, gl::STREAM_DRAW].contains(&usage) && !core_usage {
            return Err(gl::INVALID_ENUM);
        }
        if size > MAX_UPLOAD_BYTES || bytes.as_ref().is_some_and(|b| b.len() != size) {
            return Err(gl::INVALID_VALUE);
        }
        let reservation = self.prepare_object_storage(id, Kind::Buffer, size)?;
        // Numeric bufferData is zero initialized. Owned bytes are a browser
        // copy, never the author's ArrayBuffer or an asynchronously borrowed slice.
        let data = bytes.map_or_else(|| vec![0; size], Cow::into_owned);
        // The V8 copy normally has exact capacity. Normalize other internal
        // owned callers too: a tiny logical upload must not retain spare native
        // allocation outside the storage high-water accounting.
        let data = if data.capacity() == data.len() {
            data
        } else {
            data.into_boxed_slice().into_vec()
        };
        unsafe { gl::BufferData(target, size as isize, data.as_ptr().cast(), usage) };
        self.driver_result()?;
        self.commit_object_storage(reservation)?;
        self.index_cache.remove(id);
        let object = self.objects.get_mut(id, Kind::Buffer)?;
        object.bytes = data;
        object.buffer_mirror_valid = true;
        Ok(Value::Null)
    }
}

#[cfg(test)]
mod tests;
