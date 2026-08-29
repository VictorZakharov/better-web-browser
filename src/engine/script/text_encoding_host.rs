//! Native text-decoding operations used by the Encoding API bootstrap.

use super::binding_helpers::js_string;
use super::*;

pub(super) fn text_encoding_host_call(
    operation: &str,
    args: &[JsValue],
) -> JsResult<Option<JsValue>> {
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
