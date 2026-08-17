//! Small explicit primitive codec shared by typed renderer payloads.

use super::ProtocolError;

pub(super) struct WireWriter {
    bytes: Vec<u8>,
}

impl WireWriter {
    pub(super) fn new() -> Self {
        Self { bytes: Vec::new() }
    }

    pub(super) fn finish(self) -> Vec<u8> {
        self.bytes
    }

    pub(super) fn u8(&mut self, value: u8) {
        self.bytes.push(value);
    }

    pub(super) fn bool(&mut self, value: bool) {
        self.u8(value.into());
    }

    pub(super) fn u16(&mut self, value: u16) {
        self.bytes.extend_from_slice(&value.to_le_bytes());
    }

    pub(super) fn u32(&mut self, value: u32) {
        self.bytes.extend_from_slice(&value.to_le_bytes());
    }

    pub(super) fn u64(&mut self, value: u64) {
        self.bytes.extend_from_slice(&value.to_le_bytes());
    }

    pub(super) fn u128(&mut self, value: u128) {
        self.bytes.extend_from_slice(&value.to_le_bytes());
    }

    pub(super) fn f32(&mut self, value: f32) {
        self.u32(value.to_bits());
    }

    pub(super) fn string(&mut self, value: &str) -> Result<(), ProtocolError> {
        let length = u32::try_from(value.len())
            .map_err(|_| ProtocolError::InvalidPayload("wire string length"))?;
        self.u32(length);
        self.bytes.extend_from_slice(value.as_bytes());
        Ok(())
    }

    pub(super) fn bytes(&mut self, value: &[u8]) -> Result<(), ProtocolError> {
        let length = u32::try_from(value.len())
            .map_err(|_| ProtocolError::InvalidPayload("wire byte length"))?;
        self.u32(length);
        self.bytes.extend_from_slice(value);
        Ok(())
    }
}

pub(super) struct WireReader<'a> {
    bytes: &'a [u8],
    cursor: usize,
}

impl<'a> WireReader<'a> {
    pub(super) fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, cursor: 0 }
    }

    pub(super) fn finish(self) -> Result<(), ProtocolError> {
        (self.cursor == self.bytes.len())
            .then_some(())
            .ok_or(ProtocolError::InvalidPayload("trailing wire bytes"))
    }

    fn take(&mut self, length: usize) -> Result<&'a [u8], ProtocolError> {
        let end = self
            .cursor
            .checked_add(length)
            .ok_or(ProtocolError::InvalidPayload("wire length overflow"))?;
        let value = self
            .bytes
            .get(self.cursor..end)
            .ok_or(ProtocolError::InvalidPayload("truncated wire value"))?;
        self.cursor = end;
        Ok(value)
    }

    pub(super) fn u8(&mut self) -> Result<u8, ProtocolError> {
        Ok(self.take(1)?[0])
    }

    pub(super) fn bool(&mut self) -> Result<bool, ProtocolError> {
        match self.u8()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(ProtocolError::InvalidPayload("wire boolean")),
        }
    }

    pub(super) fn u16(&mut self) -> Result<u16, ProtocolError> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().unwrap()))
    }

    pub(super) fn u32(&mut self) -> Result<u32, ProtocolError> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }

    pub(super) fn u64(&mut self) -> Result<u64, ProtocolError> {
        Ok(u64::from_le_bytes(self.take(8)?.try_into().unwrap()))
    }

    pub(super) fn u128(&mut self) -> Result<u128, ProtocolError> {
        Ok(u128::from_le_bytes(self.take(16)?.try_into().unwrap()))
    }

    pub(super) fn f32(&mut self) -> Result<f32, ProtocolError> {
        Ok(f32::from_bits(self.u32()?))
    }

    pub(super) fn string(&mut self, maximum: usize) -> Result<String, ProtocolError> {
        let length = self.u32()? as usize;
        if length > maximum {
            return Err(ProtocolError::InvalidPayload("wire string budget"));
        }
        String::from_utf8(self.take(length)?.to_vec()).map_err(|_| ProtocolError::InvalidUtf8)
    }

    pub(super) fn bytes(&mut self, maximum: usize) -> Result<Vec<u8>, ProtocolError> {
        let length = self.u32()? as usize;
        if length > maximum {
            return Err(ProtocolError::InvalidPayload("wire byte budget"));
        }
        Ok(self.take(length)?.to_vec())
    }
}
