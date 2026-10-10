//! Small, already-owned void uploads share the numeric command FIFO.
//! Larger payloads retain the ordinary bounded synchronous path.
use super::*;

const MAX_QUEUED_UPLOAD: usize = 256 * 1024;
const MAX_PENDING_BYTES: usize = 4 * 1024 * 1024;

impl Pending {
    pub(in crate::engine::webgl) fn upload_candidate(source: &str, bytes: usize) -> bool {
        if bytes > MAX_QUEUED_UPLOAD || source.len() > MAX_COMMAND_BYTES {
            return false;
        }
        let Some(tail) = source.strip_prefix("{\"op\":\"") else {
            return false;
        };
        let Some((operation, suffix)) = tail.split_once('"') else {
            return false;
        };
        // Only reply-free uploads. Native parsing and all GL shape/state checks
        // still run in order; a command string establishes no binding proof.
        suffix.starts_with([',', '}'])
            && matches!(
                operation,
                "bufferData"
                    | "bufferSubData"
                    | "texImage2D"
                    | "texSubImage2D"
                    | "texImage3D"
                    | "texSubImage3D"
                    | "compressedTexImage2D"
                    | "compressedTexSubImage2D"
                    | "compressedTexImage3D"
                    | "compressedTexSubImage3D"
            )
    }

    pub(in crate::engine::webgl) fn upload_fits(&self, bytes: usize) -> bool {
        self.upload_bytes
            .checked_add(bytes)
            .is_some_and(|total| total <= MAX_PENDING_BYTES)
    }

    pub(in crate::engine::webgl) fn push_upload(&mut self, id: u32, source: &str, bytes: Vec<u8>) {
        debug_assert!(Self::upload_candidate(source, bytes.len()));
        debug_assert!(self.upload_fits(bytes.len()) && !self.full());
        self.upload_bytes += bytes.len();
        self.commands
            .push((id, Entry::Upload(source.into(), bytes)));
    }
}

#[cfg(test)]
mod tests;
