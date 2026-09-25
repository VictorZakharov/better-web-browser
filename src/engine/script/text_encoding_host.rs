//! Native text-decoding operations used by the Encoding API bootstrap.

use super::binding_helpers::js_string;
use super::*;
use encoding_rs::{CoderResult, Decoder, DecoderResult, Encoding, REPLACEMENT};
use std::collections::HashMap;

// A streaming decoder belongs to one JS realm. Non-streaming calls do not
// retain native state, so routine TextDecoder use does not accumulate handles.
#[derive(Default)]
pub(super) struct TextDecoders {
    next_id: u32,
    active: HashMap<u32, Decoder>,
}

impl TextDecoders {
    fn insert(&mut self, decoder: Decoder) -> JsResult<u32> {
        if self.active.len() >= MAX_ACTIVE {
            return Err(JsNativeError::range()
                .with_message("Too many active decoder streams")
                .into());
        }
        self.next_id = self.next_id.wrapping_add(1).max(1);
        while self.active.contains_key(&self.next_id) {
            self.next_id = self.next_id.wrapping_add(1).max(1);
        }
        self.active.insert(self.next_id, decoder);
        Ok(self.next_id)
    }
}

const MAX_INPUT: usize = 16 * 1024 * 1024;
const MAX_OUTPUT: usize = 64 * 1024 * 1024;
const MAX_ACTIVE: usize = 256;

pub(super) fn text_encoding_host_call(
    operation: &str,
    args: &[JsValue],
    decoders: &mut TextDecoders,
) -> JsResult<Option<JsValue>> {
    if operation == "decoderLabel" {
        let label = super::binding_helpers::argument_string(args, 1)?;
        let encoding = decoder_encoding(&label)?;
        return Ok(Some(js_string(encoding.name().to_ascii_lowercase())));
    }
    if operation == "decoderDecode" {
        return decode_call(args, decoders).map(Some);
    }
    if operation == "decoderCreate" {
        let label = super::binding_helpers::argument_string(args, 1)?;
        let encoding = decoder_encoding(&label)?;
        let ignore_bom = boolean_argument(args, 2);
        let decoder = if ignore_bom {
            encoding.new_decoder_without_bom_handling()
        } else {
            encoding.new_decoder_with_bom_removal()
        };
        return Ok(Some(JsValue::from(decoders.insert(decoder)?)));
    }
    if operation == "fileReadText" {
        let bytes = typed_array_bytes(args, 1)?;
        let label = super::binding_helpers::argument_string(args, 2)?;
        let charset = super::binding_helpers::argument_string(args, 3)?;
        // File API packaging: explicit label, MIME charset, then UTF-8.
        // Encoding's decode algorithm gives a BOM precedence over that fallback.
        let encoding = encoding_rs::Encoding::for_label(label.as_bytes())
            .or_else(|| encoding_rs::Encoding::for_label(charset.as_bytes()))
            .unwrap_or(encoding_rs::UTF_8);
        let (text, _, _) = encoding.decode(&bytes);
        return Ok(Some(js_string(text.into_owned())));
    }
    if operation != "utf8Decode" {
        return Ok(None);
    }
    let mut bytes = typed_array_bytes(args, 2)?;
    let input = typed_array_bytes(args, 1)?;
    bytes.extend_from_slice(&input);
    let stream = boolean_argument(args, 3);
    let fatal = boolean_argument(args, 4);
    let ignore_bom = boolean_argument(args, 5);
    let bom_seen = boolean_argument(args, 6);
    let (text, pending, bom_seen) = decode_utf8(&bytes, stream, fatal, ignore_bom, bom_seen)
        .map_err(|()| JsNativeError::typ().with_message("The encoded data was not valid UTF-8"))?;
    let result = JsValue::Array(vec![
        js_string(text),
        JsValue::Bytes(pending),
        JsValue::from(bom_seen),
    ]);
    Ok(Some(result))
}

fn decoder_encoding(label: &str) -> JsResult<&'static Encoding> {
    let encoding = Encoding::for_label(label.as_bytes()).filter(|value| *value != REPLACEMENT);
    encoding.ok_or_else(|| {
        JsNativeError::range()
            .with_message("Unsupported encoding label")
            .into()
    })
}

fn decode_call(args: &[JsValue], contexts: &mut TextDecoders) -> JsResult<JsValue> {
    let label = super::binding_helpers::argument_string(args, 1)?;
    let encoding = decoder_encoding(&label)?;
    let bytes = typed_array_bytes(args, 2)?;
    if bytes.len() > MAX_INPUT {
        return Err(JsNativeError::range()
            .with_message("Decoder input exceeds the per-call limit")
            .into());
    }
    let stream = boolean_argument(args, 3);
    let fatal = boolean_argument(args, 4);
    let ignore_bom = boolean_argument(args, 5);
    let id = args.get(6).and_then(JsValue::as_number).unwrap_or(0.0) as u32;
    let mut decoder = if id == 0 {
        if ignore_bom {
            encoding.new_decoder_without_bom_handling()
        } else {
            encoding.new_decoder_with_bom_removal()
        }
    } else {
        contexts.active.remove(&id).ok_or_else(|| {
            JsNativeError::typ().with_message("Decoder stream is no longer active")
        })?
    };
    // A non-final empty chunk does not run the underlying decoder. In
    // particular, encoding_rs would otherwise lose a pending multibyte lead.
    let result = if stream && bytes.is_empty() {
        Ok(String::new())
    } else {
        decode_bytes(&mut decoder, &bytes, !stream, fatal)
    };
    let next_id = if stream {
        if id == 0 {
            contexts.insert(decoder)?
        } else {
            contexts.active.insert(id, decoder);
            id
        }
    } else {
        0
    };
    let text = result.map_err(|message| JsNativeError::typ().with_message(message))?;
    Ok(JsValue::Array(vec![
        js_string(text),
        JsValue::from(next_id),
    ]))
}

fn decode_bytes(
    decoder: &mut Decoder,
    bytes: &[u8],
    last: bool,
    fatal: bool,
) -> Result<String, &'static str> {
    let capacity = if fatal {
        decoder.max_utf8_buffer_length_without_replacement(bytes.len())
    } else {
        decoder.max_utf8_buffer_length(bytes.len())
    }
    .ok_or("Decoder output is too large")?;
    if capacity > MAX_OUTPUT {
        return Err("Decoder output exceeds the per-call limit");
    }
    let mut text = String::with_capacity(capacity.max(1));
    if fatal {
        let (result, read) = decoder.decode_to_string_without_replacement(bytes, &mut text, last);
        if read != bytes.len() || result != DecoderResult::InputEmpty {
            return Err("The encoded data is invalid");
        }
    } else {
        let (result, read, _) = decoder.decode_to_string(bytes, &mut text, last);
        if read != bytes.len() || result != CoderResult::InputEmpty {
            return Err("Decoder output is too large");
        }
    }
    Ok(text)
}

fn boolean_argument(args: &[JsValue], index: usize) -> bool {
    args.get(index)
        .and_then(JsValue::as_boolean)
        .unwrap_or(false)
}

fn typed_array_bytes(args: &[JsValue], index: usize) -> JsResult<Vec<u8>> {
    args.get(index)
        .and_then(JsValue::as_bytes)
        .map(<[u8]>::to_vec)
        .ok_or_else(|| {
            JsNativeError::typ()
                .with_message("decoder input is not a Uint8Array")
                .into()
        })
}

fn decode_utf8(
    bytes: &[u8],
    stream: bool,
    fatal: bool,
    ignore_bom: bool,
    mut bom_seen: bool,
) -> Result<(String, Vec<u8>, bool), ()> {
    let mut output = String::with_capacity(bytes.len());
    let mut index = 0;
    let mut pending = Vec::new();
    while index < bytes.len() {
        let first = bytes[index];
        if first <= 0x7f {
            emit(first.into(), &mut output, ignore_bom, &mut bom_seen);
            index += 1;
            continue;
        }
        let (needed, mut scalar, minimum) = match first {
            0xc2..=0xdf => (1, u32::from(first & 0x1f), 0x80),
            0xe0..=0xef => (2, u32::from(first & 0x0f), 0x800),
            0xf0..=0xf4 => (3, u32::from(first & 0x07), 0x10000),
            _ => {
                decode_error(fatal, &mut output, ignore_bom, &mut bom_seen)?;
                index += 1;
                continue;
            }
        };
        if index + needed >= bytes.len() {
            if stream {
                pending.extend_from_slice(&bytes[index..]);
            } else {
                decode_error(fatal, &mut output, ignore_bom, &mut bom_seen)?;
            }
            break;
        }
        let mut valid = true;
        for offset in 1..=needed {
            let continuation = bytes[index + offset];
            if continuation & 0xc0 != 0x80 {
                valid = false;
                break;
            }
            scalar = (scalar << 6) | u32::from(continuation & 0x3f);
        }
        if !valid || scalar < minimum || scalar > 0x10ffff || (0xd800..=0xdfff).contains(&scalar) {
            decode_error(fatal, &mut output, ignore_bom, &mut bom_seen)?;
            index += 1;
            continue;
        }
        emit(scalar, &mut output, ignore_bom, &mut bom_seen);
        index += needed + 1;
    }
    Ok((output, pending, bom_seen))
}

fn decode_error(
    fatal: bool,
    output: &mut String,
    ignore_bom: bool,
    bom_seen: &mut bool,
) -> Result<(), ()> {
    if fatal {
        return Err(());
    }
    emit(0xfffd, output, ignore_bom, bom_seen);
    Ok(())
}

fn emit(scalar: u32, output: &mut String, ignore_bom: bool, bom_seen: &mut bool) {
    if !*bom_seen {
        *bom_seen = true;
        if !ignore_bom && scalar == 0xfeff {
            return;
        }
    }
    output.push(char::from_u32(scalar).expect("validated Unicode scalar"));
}

#[cfg(test)]
mod tests {
    use super::decode_utf8;

    #[test]
    fn preserves_split_scalars_between_streaming_calls() {
        let (first, pending, seen) = decode_utf8(&[0xe2, 0x82], true, false, false, false).unwrap();
        assert_eq!(first, "");
        assert_eq!(pending, [0xe2, 0x82]);
        let mut remainder = pending;
        remainder.push(0xac);
        let (second, pending, _) = decode_utf8(&remainder, false, false, false, seen).unwrap();
        assert_eq!(second, "€");
        assert!(pending.is_empty());
    }

    #[test]
    fn replaces_invalid_sequences_and_filters_the_initial_bom() {
        let (text, pending, _) = decode_utf8(
            &[0xef, 0xbb, 0xbf, 0xe2, b'(', 0xa1],
            false,
            false,
            false,
            false,
        )
        .unwrap();
        assert_eq!(text, "�(�");
        assert!(pending.is_empty());
    }
}
