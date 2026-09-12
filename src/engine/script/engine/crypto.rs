//! Shared Window/Worker randomness bridge. No script PRNG or insecure OS fallback.

use super::value::{JsNativeError, JsResult, JsValue};

const RANDOM_BYTE_LIMIT: usize = 65_536;

pub(super) fn random_bytes(args: &[JsValue]) -> JsResult<JsValue> {
    random_bytes_with(args, |bytes| {
        getrandom::fill(bytes).map_err(|error| error.to_string())
    })
}

fn random_bytes_with(
    args: &[JsValue],
    fill: impl FnOnce(&mut [u8]) -> Result<(), String>,
) -> JsResult<JsValue> {
    let length = args.get(1).and_then(JsValue::as_number).unwrap_or(f64::NAN);
    // The host boundary is callable by hostile script: enforce bounds before allocation too.
    if !length.is_finite()
        || length.fract() != 0.0
        || !(0.0..=RANDOM_BYTE_LIMIT as f64).contains(&length)
    {
        return Err(JsNativeError::range()
            .with_message("random byte count must be an integer in 0..=65536")
            .into());
    }
    let mut bytes = vec![0; length as usize];
    if !bytes.is_empty() {
        fill(&mut bytes).map_err(|error| {
            JsNativeError::error()
                .with_message(format!("operating-system random source failed: {error}"))
        })?;
    }
    // Never publish an unfilled or partially filled buffer on a provider failure.
    Ok(JsValue::Bytes(bytes))
}

pub(super) fn trustworthy_url(value: &str) -> bool {
    let Ok(url) = url::Url::parse(value) else {
        return false;
    };
    if url.scheme() == "file" {
        return true;
    }
    // Current realms are top-level documents or their same-origin dedicated workers. Blob
    // URLs inherit their tuple origin; opaque origins do not acquire secure-only UUID access.
    // https://www.w3.org/TR/secure-contexts/#is-origin-trustworthy
    let url::Origin::Tuple(scheme, host, _) = url.origin() else {
        return false;
    };
    if matches!(scheme.as_str(), "https" | "wss") {
        return true;
    }
    match host {
        url::Host::Ipv4(address) => address.is_loopback(),
        url::Host::Ipv6(address) => address.is_loopback(),
        // Do not trust localhost names until the transport enforces loopback-only resolution.
        url::Host::Domain(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn arguments(length: f64) -> [JsValue; 2] {
        [
            JsValue::String("cryptoRandomBytes".into()),
            JsValue::Number(length),
        ]
    }

    #[test]
    fn crypto_host_rejects_invalid_lengths_before_calling_provider() {
        for length in [-1.0, 0.5, 65537.0, f64::NAN, f64::INFINITY] {
            assert!(random_bytes_with(&arguments(length), |_| panic!("provider called")).is_err());
        }
        assert!(random_bytes_with(&[], |_| panic!("provider called")).is_err());
        assert_eq!(
            random_bytes_with(&arguments(0.0), |_| panic!("provider called")).unwrap(),
            JsValue::Bytes(vec![])
        );
    }

    #[test]
    fn crypto_host_fails_closed_and_preserves_full_byte_range() {
        let args = arguments(256.0);
        let result = random_bytes_with(&args, |bytes| {
            for (index, byte) in bytes.iter_mut().enumerate() {
                *byte = index as u8;
            }
            Ok(())
        })
        .unwrap();
        assert_eq!(result, JsValue::Bytes((0..=255).collect()));
        assert!(
            random_bytes_with(&args, |bytes| {
                bytes[0] = 99;
                Err("injected provider failure".into())
            })
            .unwrap_err()
            .message
            .contains("injected provider failure")
        );
    }

    #[test]
    fn crypto_uuid_trust_uses_origin_not_url_prefix() {
        for url in [
            "https://example.com/",
            "http://127.9.8.7/",
            "http://[::1]/",
            "file:///tmp/test.html",
            "blob:https://example.com/id",
        ] {
            assert!(trustworthy_url(url), "{url}");
        }
        for url in [
            "http://example.com/",
            "http://localhost.example/",
            "http://localhost/",
            "http://a.localhost./",
            "http://192.168.1.1/",
            "data:text/html,https://example.com/",
            "about:blank",
            "blob:null/id",
            "https://",
        ] {
            assert!(!trustworthy_url(url), "{url}");
        }
    }
}
