pub(crate) fn decode_base64(input: &str) -> Result<Vec<u8>, String> {
    if !input.len().is_multiple_of(4) {
        return Err("request body has invalid base64 length".into());
    }
    let mut output = Vec::with_capacity(input.len() / 4 * 3);
    for (chunk_index, chunk) in input.as_bytes().chunks_exact(4).enumerate() {
        let last = chunk_index + 1 == input.len() / 4;
        let padding = usize::from(chunk[3] == b'=') + usize::from(chunk[2] == b'=');
        if (padding > 0 && !last) || (chunk[2] == b'=' && chunk[3] != b'=') {
            return Err("request body has invalid base64 padding".into());
        }
        let a = base64_value(chunk[0])?;
        let b = base64_value(chunk[1])?;
        let c = if chunk[2] == b'=' {
            0
        } else {
            base64_value(chunk[2])?
        };
        let d = if chunk[3] == b'=' {
            0
        } else {
            base64_value(chunk[3])?
        };
        let bits = (u32::from(a) << 18) | (u32::from(b) << 12) | (u32::from(c) << 6) | u32::from(d);
        output.push((bits >> 16) as u8);
        if padding < 2 {
            output.push((bits >> 8) as u8);
        }
        if padding == 0 {
            output.push(bits as u8);
        }
    }
    Ok(output)
}

fn base64_value(byte: u8) -> Result<u8, String> {
    match byte {
        b'A'..=b'Z' => Ok(byte - b'A'),
        b'a'..=b'z' => Ok(byte - b'a' + 26),
        b'0'..=b'9' => Ok(byte - b'0' + 52),
        b'+' => Ok(62),
        b'/' => Ok(63),
        _ => Err("request body contains invalid base64 data".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_standard_padded_base64() {
        assert_eq!(decode_base64("UnVzdA==").unwrap(), b"Rust");
        assert_eq!(decode_base64("SGVsbG8h").unwrap(), b"Hello!");
        assert!(decode_base64("abc").is_err());
        assert!(decode_base64("AA=A").is_err());
    }
}
