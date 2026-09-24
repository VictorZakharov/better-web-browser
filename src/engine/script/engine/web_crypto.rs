//! Native Web Crypto primitives. JS owns Web IDL conversion, key usages and extractability;
//! the host independently bounds allocations and never substitutes a non-cryptographic fallback.

mod cng;

use super::value::{JsNativeError, JsResult, JsValue};

const MAX_INPUT: usize = 16 * 1024 * 1024;
// PBKDF2 is synchronous in this isolated renderer. Bound CPU work as well as
// allocation so an untrusted page cannot monopolize its script thread.
const MAX_PBKDF2_ITERATIONS: u32 = 1_000_000;

pub(super) fn dispatch(operation: &str, args: &[JsValue]) -> JsResult<JsValue> {
    let bytes = |at| -> JsResult<&[u8]> {
        let value = args
            .get(at)
            .and_then(JsValue::as_bytes)
            .ok_or_else(invalid_input)?;
        if value.len() > MAX_INPUT {
            return Err(invalid_input());
        }
        Ok(value)
    };
    let name = |at| -> JsResult<&str> {
        match args.get(at) {
            Some(JsValue::String(value)) => Ok(value.as_str()),
            _ => Err(invalid_input()),
        }
    };
    let number = |at| -> JsResult<u32> {
        let value = args
            .get(at)
            .and_then(JsValue::as_number)
            .ok_or_else(invalid_input)?;
        if !value.is_finite() || value.fract() != 0.0 || !(0.0..=u32::MAX as f64).contains(&value) {
            return Err(invalid_input());
        }
        Ok(value as u32)
    };
    if operation.starts_with("cryptoSubtleEc") {
        let family = name(1)?;
        let curve = name(2)?;
        let native = match operation {
            "cryptoSubtleEcGenerate" => {
                let (public, private) = cng::ec::generate(family, curve)
                    .map_err(|message| JsNativeError::error().with_message(message))?;
                return Ok(JsValue::Array(vec![
                    JsValue::Bytes(public),
                    JsValue::Bytes(private),
                ]));
            }
            "cryptoSubtleEcValidate" => {
                cng::ec::validate(family, curve, bytes(3)?)
                    .map_err(|message| JsNativeError::error().with_message(message))?;
                return Ok(JsValue::Boolean(true));
            }
            "cryptoSubtleEcDerive" => cng::ec::derive(curve, bytes(3)?, bytes(4)?),
            "cryptoSubtleEcSign" => cng::ec::sign(curve, name(3)?, bytes(4)?, bytes(5)?),
            "cryptoSubtleEcVerify" => {
                let verified = cng::ec::verify(curve, name(3)?, bytes(4)?, bytes(5)?, bytes(6)?)
                    .map_err(|message| JsNativeError::error().with_message(message))?;
                return Ok(JsValue::Boolean(verified));
            }
            _ => return Err(invalid_input()),
        };
        return native
            .map(JsValue::Bytes)
            .map_err(|message| JsNativeError::error().with_message(message).into());
    }
    if operation.starts_with("cryptoSubtleRsa") {
        let boolean = |at| match args.get(at) {
            Some(JsValue::Boolean(value)) => Ok(*value),
            _ => Err(invalid_input()),
        };
        return match operation {
            "cryptoSubtleRsaGenerate" => {
                let (public, private) = cng::rsa::generate(number(1)?)
                    .map_err(|message| JsNativeError::error().with_message(message))?;
                Ok(JsValue::Array(vec![
                    JsValue::Bytes(public),
                    JsValue::Bytes(private),
                ]))
            }
            "cryptoSubtleRsaValidate" => {
                cng::rsa::validate(bytes(2)?, boolean(1)?)
                    .map_err(|message| JsNativeError::error().with_message(message))?;
                Ok(JsValue::Boolean(true))
            }
            "cryptoSubtleRsaOaep" => {
                cng::rsa::oaep(boolean(1)?, name(2)?, bytes(3)?, bytes(4)?, bytes(5)?)
                    .map(JsValue::Bytes)
                    .map_err(|message| JsNativeError::error().with_message(message).into())
            }
            "cryptoSubtleRsaSignature" => {
                let verify = boolean(1)?;
                let result = cng::rsa::signature(
                    verify,
                    boolean(2)?,
                    name(3)?,
                    number(4)?,
                    bytes(5)?,
                    bytes(6)?,
                    bytes(7)?,
                )
                .map_err(|message| JsNativeError::error().with_message(message))?;
                Ok(if verify {
                    JsValue::Boolean(result.is_some())
                } else {
                    JsValue::Bytes(result.ok_or_else(invalid_input)?)
                })
            }
            _ => Err(invalid_input()),
        };
    }
    let result = match operation {
        "cryptoSubtleDigest" => cng::hash(name(1)?, None, bytes(2)?),
        "cryptoSubtleHmac" => cng::hash(name(1)?, Some(bytes(2)?), bytes(3)?),
        "cryptoSubtlePbkdf2" => {
            let length = number(5)? as usize;
            let iterations = number(4)?;
            if length > MAX_INPUT || iterations > MAX_PBKDF2_ITERATIONS {
                return Err(invalid_input());
            }
            cng::pbkdf2(name(1)?, bytes(2)?, bytes(3)?, iterations, length)
        }
        "cryptoSubtleHkdf" => {
            let length = number(5)? as usize;
            if length > MAX_INPUT {
                return Err(invalid_input());
            }
            cng::hkdf(name(1)?, bytes(2)?, bytes(3)?, bytes(4)?, length)
        }
        "cryptoSubtleAesCbc" => {
            let decrypt = match args.get(1) {
                Some(JsValue::Boolean(value)) => *value,
                _ => return Err(invalid_input()),
            };
            cng::aes_cbc(decrypt, bytes(2)?, bytes(3)?, bytes(4)?)
        }
        "cryptoSubtleAesCtr" => cng::aes_ctr(bytes(1)?, bytes(2)?, number(3)?, bytes(4)?),
        "cryptoSubtleAesGcm" => {
            let decrypt = match args.get(1) {
                Some(JsValue::Boolean(value)) => *value,
                _ => return Err(invalid_input()),
            };
            cng::aes_gcm(
                decrypt,
                bytes(2)?,
                bytes(3)?,
                bytes(4)?,
                bytes(5)?,
                number(6)? as usize,
            )
        }
        "cryptoSubtleAesKw" => {
            let unwrap = match args.get(1) {
                Some(JsValue::Boolean(value)) => *value,
                _ => return Err(invalid_input()),
            };
            if unwrap {
                cng::kw::unwrap(bytes(2)?, bytes(3)?)
            } else {
                cng::kw::wrap(bytes(2)?, bytes(3)?)
            }
        }
        _ => return Err(invalid_input()),
    };
    result
        .map(JsValue::Bytes)
        .map_err(|message| JsNativeError::error().with_message(message).into())
}

fn invalid_input() -> super::value::JsError {
    JsNativeError::typ()
        .with_message("invalid Web Crypto host input")
        .into()
}

#[cfg(test)]
mod tests;
