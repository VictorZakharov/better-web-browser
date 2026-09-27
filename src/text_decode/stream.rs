//! Stateful document decoding. Encoding declarations come from the real HTML tree builder,
//! not a search for '<meta' inside comments, scripts, or incomplete network chunks.
//! https://html.spec.whatwg.org/multipage/parsing.html#changing-the-encoding-while-parsing
use super::{DecodedText, charset_from_content_type};
use crate::limits::MAX_RESPONSE_BODY_BYTES;
use encoding_rs::{Decoder, Encoding, UTF_8, UTF_16BE, UTF_16LE, WINDOWS_1252, X_USER_DEFINED};

pub(crate) struct DocumentDecoder {
    bytes: Vec<u8>,
    decoder: Option<Decoder>,
    transport: Option<&'static Encoding>,
    encoding: &'static Encoding,
    certain: bool,
    ended: bool,
}

impl DocumentDecoder {
    pub(crate) fn new(content_type: &str) -> Self {
        Self {
            bytes: Vec::new(),
            decoder: None,
            transport: charset_from_content_type(content_type)
                .and_then(|label| Encoding::for_label(label.as_bytes())),
            encoding: UTF_8,
            certain: false,
            ended: false,
        }
    }

    pub(crate) fn push(&mut self, bytes: &[u8], eof: bool) -> Result<Option<DecodedText>, String> {
        if self.ended {
            return Err("document bytes arrived after EOF".into());
        }
        if self.bytes.len().saturating_add(bytes.len()) > MAX_RESPONSE_BODY_BYTES {
            return Err("document response exceeded its byte limit".into());
        }
        self.bytes.extend_from_slice(bytes);
        let input = if self.decoder.is_none() {
            // Only wait while the available bytes could still be the prefix of a BOM.
            if !eof
                && [[0xef, 0xbb, 0xbf].as_slice(), &[0xff, 0xfe], &[0xfe, 0xff]]
                    .iter()
                    .any(|bom| self.bytes.len() < bom.len() && bom.starts_with(&self.bytes))
            {
                return Ok(None);
            }
            let bom = Encoding::for_bom(&self.bytes).map(|(encoding, _)| encoding);
            self.encoding = bom.or(self.transport).unwrap_or(UTF_8);
            self.certain = bom.is_some() || self.transport.is_some();
            self.decoder = Some(self.encoding.new_decoder());
            self.bytes.as_slice()
        } else {
            bytes
        };
        let text = decode(
            self.decoder.as_mut().expect("initialized decoder"),
            input,
            eof,
        );
        self.ended = eof;
        Ok(Some(DecodedText {
            text,
            encoding: self.encoding.name(),
        }))
    }

    pub(crate) fn change_encoding(&mut self, label: &str) -> Option<DecodedText> {
        if self.certain {
            return None;
        }
        let mut encoding = Encoding::for_label(label.as_bytes())?;
        self.certain = true;
        if self.encoding == UTF_16BE || self.encoding == UTF_16LE {
            return None;
        }
        if encoding == UTF_16BE || encoding == UTF_16LE {
            encoding = UTF_8;
        }
        if encoding == X_USER_DEFINED {
            encoding = WINDOWS_1252;
        }
        if encoding == self.encoding {
            return None;
        }
        self.encoding = encoding;
        let mut decoder = encoding.new_decoder_without_bom_handling();
        let text = decode(&mut decoder, &self.bytes, self.ended);
        self.decoder = Some(decoder);
        Some(DecodedText {
            text,
            encoding: encoding.name(),
        })
    }

    pub(crate) fn ended(&self) -> bool {
        self.ended
    }
}

fn decode(decoder: &mut Decoder, bytes: &[u8], eof: bool) -> String {
    let capacity = decoder
        .max_utf8_buffer_length(bytes.len())
        .expect("bounded input");
    let mut output = String::with_capacity(capacity);
    let (result, read, _) = decoder.decode_to_string(bytes, &mut output, eof);
    debug_assert_eq!(result, encoding_rs::CoderResult::InputEmpty);
    debug_assert_eq!(read, bytes.len());
    output
}

#[cfg(test)]
mod tests;
