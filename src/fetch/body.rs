//! In-memory body storage with streaming reads and a hard byte budget.

use super::FetchError;
use std::io::{self, Read};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Body {
    bytes: Vec<u8>,
    cursor: usize,
    limit: usize,
}

impl Body {
    pub fn empty(limit: usize) -> Self {
        Self {
            bytes: Vec::new(),
            cursor: 0,
            limit,
        }
    }

    pub fn from_bytes(bytes: Vec<u8>) -> Self {
        let limit = bytes.len();
        Self {
            bytes,
            cursor: 0,
            limit,
        }
    }

    pub fn push(&mut self, chunk: &[u8]) -> Result<(), FetchError> {
        if self.bytes.len().saturating_add(chunk.len()) > self.limit {
            return Err(FetchError::body_too_large(self.limit));
        }
        self.bytes.extend_from_slice(chunk);
        Ok(())
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }

    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    pub fn rewind(&mut self) {
        self.cursor = 0;
    }

    pub fn chunks(&self, chunk_size: usize) -> impl Iterator<Item = &[u8]> {
        self.bytes.chunks(chunk_size.max(1))
    }
}

impl Read for Body {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let remaining = &self.bytes[self.cursor..];
        let count = remaining.len().min(buffer.len());
        buffer[..count].copy_from_slice(&remaining[..count]);
        self.cursor += count;
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enforces_budget_and_supports_incremental_reads() {
        let mut body = Body::empty(4);
        body.push(b"ab").unwrap();
        body.push(b"cd").unwrap();
        assert!(body.push(b"e").is_err());

        let mut output = [0_u8; 3];
        assert_eq!(body.read(&mut output).unwrap(), 3);
        assert_eq!(&output, b"abc");
        assert_eq!(body.read(&mut output).unwrap(), 1);
    }
}
