//! Per-realm streaming DEFLATE codecs for the Compression Streams API.

use super::super::binding_helpers::{argument_id, argument_string};
use super::super::{JsNativeError, JsResult, JsValue};
use flate2::write::{DeflateEncoder, GzDecoder, GzEncoder, ZlibEncoder};
use flate2::{Compression, Decompress, FlushDecompress, Status};
use std::collections::HashMap;
use std::io::{self, Write};

const MAX_CONTEXTS: usize = 64;
const MAX_INPUT_CHUNK: usize = 16 * 1024 * 1024;
const MAX_OUTPUT_CHUNK: usize = 64 * 1024 * 1024;

#[derive(Default)]
pub(in crate::engine::script) struct CompressionStreams {
    next_id: u32,
    contexts: HashMap<u32, Codec>,
}

struct BoundedOutput {
    bytes: Vec<u8>,
}

impl BoundedOutput {
    fn new() -> Self {
        Self { bytes: Vec::new() }
    }

    fn take(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.bytes)
    }
}

impl Write for BoundedOutput {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self.bytes.len().saturating_add(bytes.len()) > MAX_OUTPUT_CHUNK {
            return Err(io::Error::other("compression output chunk limit exceeded"));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

struct Inflate {
    decoder: Decompress,
    ended: bool,
}

impl Inflate {
    fn new(zlib_header: bool) -> Self {
        Self {
            decoder: Decompress::new(zlib_header),
            ended: false,
        }
    }

    fn process(&mut self, input: &[u8], finish: bool) -> io::Result<Vec<u8>> {
        if self.ended {
            return if input.is_empty() {
                Ok(Vec::new())
            } else {
                Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "trailing compressed data",
                ))
            };
        }
        let mut output = Vec::new();
        let mut offset = 0;
        loop {
            let mut buffer = [0_u8; 32 * 1024];
            let before_in = self.decoder.total_in();
            let before_out = self.decoder.total_out();
            let flush = if finish {
                FlushDecompress::Finish
            } else {
                FlushDecompress::None
            };
            let status = self
                .decoder
                .decompress(&input[offset..], &mut buffer, flush)
                .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "invalid DEFLATE data"))?;
            let consumed = (self.decoder.total_in() - before_in) as usize;
            let produced = (self.decoder.total_out() - before_out) as usize;
            offset += consumed;
            if output.len().saturating_add(produced) > MAX_OUTPUT_CHUNK {
                return Err(io::Error::other(
                    "decompression output chunk limit exceeded",
                ));
            }
            output.extend_from_slice(&buffer[..produced]);
            if status == Status::StreamEnd {
                self.ended = true;
                if offset != input.len() {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "trailing compressed data",
                    ));
                }
                break;
            }
            if consumed == 0 && produced == 0 {
                if finish {
                    return Err(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "truncated compressed stream",
                    ));
                }
                break;
            }
            if offset == input.len() && produced < buffer.len() && !finish {
                break;
            }
        }
        Ok(output)
    }
}

enum Codec {
    GzipEncoder(GzEncoder<BoundedOutput>),
    ZlibEncoder(ZlibEncoder<BoundedOutput>),
    RawEncoder(DeflateEncoder<BoundedOutput>),
    GzipDecoder(GzDecoder<BoundedOutput>),
    ZlibDecoder(Inflate),
    RawDecoder(Inflate),
}

impl Codec {
    fn new(mode: &str, format: &str) -> Option<Self> {
        let output = BoundedOutput::new();
        let level = Compression::default();
        match (mode, format) {
            ("compress", "gzip") => Some(Self::GzipEncoder(GzEncoder::new(output, level))),
            ("compress", "deflate") => Some(Self::ZlibEncoder(ZlibEncoder::new(output, level))),
            ("compress", "deflate-raw") => {
                Some(Self::RawEncoder(DeflateEncoder::new(output, level)))
            }
            ("decompress", "gzip") => Some(Self::GzipDecoder(GzDecoder::new(output))),
            ("decompress", "deflate") => Some(Self::ZlibDecoder(Inflate::new(true))),
            ("decompress", "deflate-raw") => Some(Self::RawDecoder(Inflate::new(false))),
            _ => None,
        }
    }

    fn write(&mut self, input: &[u8]) -> io::Result<Vec<u8>> {
        match self {
            Self::GzipEncoder(codec) => {
                codec.write_all(input)?;
                codec.flush()?;
                Ok(codec.get_mut().take())
            }
            Self::ZlibEncoder(codec) => {
                codec.write_all(input)?;
                codec.flush()?;
                Ok(codec.get_mut().take())
            }
            Self::RawEncoder(codec) => {
                codec.write_all(input)?;
                codec.flush()?;
                Ok(codec.get_mut().take())
            }
            Self::GzipDecoder(codec) => {
                codec.write_all(input)?;
                codec.flush()?;
                Ok(codec.get_mut().take())
            }
            Self::ZlibDecoder(codec) | Self::RawDecoder(codec) => codec.process(input, false),
        }
    }

    fn finish(self) -> io::Result<Vec<u8>> {
        match self {
            Self::GzipEncoder(codec) => Ok(codec.finish()?.bytes),
            Self::ZlibEncoder(codec) => Ok(codec.finish()?.bytes),
            Self::RawEncoder(codec) => Ok(codec.finish()?.bytes),
            Self::GzipDecoder(codec) => Ok(codec.finish()?.bytes),
            Self::ZlibDecoder(mut codec) | Self::RawDecoder(mut codec) => codec.process(&[], true),
        }
    }
}

pub(in crate::engine::script) fn dispatch(
    operation: &str,
    args: &[JsValue],
    streams: &mut CompressionStreams,
) -> JsResult<Option<JsValue>> {
    match operation {
        "compressionCreate" => {
            if streams.contexts.len() >= MAX_CONTEXTS {
                return Err(JsNativeError::range()
                    .with_message("too many active compression streams")
                    .into());
            }
            let mode = argument_string(args, 1)?;
            let format = argument_string(args, 2)?;
            let codec = Codec::new(&mode, &format).ok_or_else(|| {
                JsNativeError::typ().with_message("unsupported compression format")
            })?;
            streams.next_id = streams.next_id.checked_add(1).ok_or_else(|| {
                JsNativeError::range().with_message("compression stream identifiers exhausted")
            })?;
            streams.contexts.insert(streams.next_id, codec);
            Ok(Some(JsValue::from(streams.next_id)))
        }
        "compressionWrite" => {
            let id = argument_id(args, 1);
            let bytes = args.get(2).and_then(JsValue::as_bytes).ok_or_else(|| {
                JsNativeError::typ().with_message("compression input must be a Uint8Array")
            })?;
            if bytes.len() > MAX_INPUT_CHUNK {
                return Err(JsNativeError::range()
                    .with_message("compression input chunk limit exceeded")
                    .into());
            }
            let codec = streams.contexts.get_mut(&id).ok_or_else(|| {
                JsNativeError::typ().with_message("compression stream is not active")
            })?;
            match codec.write(bytes) {
                Ok(output) => Ok(Some(JsValue::Bytes(output))),
                Err(error) => {
                    streams.contexts.remove(&id);
                    Err(JsNativeError::typ().with_message(error.to_string()).into())
                }
            }
        }
        "compressionFinish" => {
            let id = argument_id(args, 1);
            let codec = streams.contexts.remove(&id).ok_or_else(|| {
                JsNativeError::typ().with_message("compression stream is not active")
            })?;
            let output = codec
                .finish()
                .map_err(|error| JsNativeError::typ().with_message(error.to_string()))?;
            Ok(Some(JsValue::Bytes(output)))
        }
        "compressionDrop" => {
            streams.contexts.remove(&argument_id(args, 1));
            Ok(Some(JsValue::undefined()))
        }
        _ => Ok(None),
    }
}

#[cfg(test)]
#[path = "compression_host_tests.rs"]
mod tests;
