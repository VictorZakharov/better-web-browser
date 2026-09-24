//! Subresource Integrity metadata and byte verification, before consumers decode a response.
//! https://www.w3.org/TR/SRI/#does-response-match-metadatalist

use base64::Engine as _;
use base64::engine::general_purpose::{STANDARD, STANDARD_NO_PAD};
use sha2::{Digest, Sha256, Sha384, Sha512};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Algorithm {
    Sha256,
    Sha384,
    Sha512,
}

#[derive(Debug, PartialEq, Eq)]
pub enum IntegrityError {
    IneligibleResponse,
    DigestMismatch,
}

impl std::fmt::Display for IntegrityError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::IneligibleResponse => formatter.write_str(
                "Subresource Integrity requires a same-origin or CORS-approved response",
            ),
            Self::DigestMismatch => formatter.write_str("Subresource Integrity digest mismatch"),
        }
    }
}

/// Invalid and unknown metadata are ignored for forward compatibility. Among usable hashes,
/// only the strongest supported algorithm participates; any one matching digest suffices.
pub fn verify(metadata: &str, bytes: &[u8], eligible: bool) -> Result<(), IntegrityError> {
    let entries = metadata
        .split_ascii_whitespace()
        .filter_map(parse_entry)
        .collect::<Vec<_>>();
    let Some(strongest) = entries.iter().map(|(algorithm, _)| *algorithm).max() else {
        return Ok(());
    };
    if !eligible {
        return Err(IntegrityError::IneligibleResponse);
    }
    let actual: Vec<u8> = match strongest {
        Algorithm::Sha256 => Sha256::digest(bytes).to_vec(),
        Algorithm::Sha384 => Sha384::digest(bytes).to_vec(),
        Algorithm::Sha512 => Sha512::digest(bytes).to_vec(),
    };
    if entries
        .iter()
        .any(|(algorithm, expected)| *algorithm == strongest && expected == &actual)
    {
        Ok(())
    } else {
        Err(IntegrityError::DigestMismatch)
    }
}

fn parse_entry(token: &str) -> Option<(Algorithm, Vec<u8>)> {
    let (hash, _) = token.split_once('?').unwrap_or((token, ""));
    let (algorithm, encoded) = hash.split_once('-')?;
    let algorithm = match algorithm.to_ascii_lowercase().as_str() {
        "sha256" => Algorithm::Sha256,
        "sha384" => Algorithm::Sha384,
        "sha512" => Algorithm::Sha512,
        _ => return None,
    };
    // CSP's base64-value also admits the URL-safe alphabet. Normalize it without accepting
    // arbitrary padding or whitespace inside one metadata token.
    let normalized = encoded.replace('-', "+").replace('_', "/");
    let digest = STANDARD
        .decode(normalized.as_bytes())
        .or_else(|_| STANDARD_NO_PAD.decode(normalized.as_bytes()))
        .ok()?;
    let length = match algorithm {
        Algorithm::Sha256 => 32,
        Algorithm::Sha384 => 48,
        Algorithm::Sha512 => 64,
    };
    (digest.len() == length).then_some((algorithm, digest))
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::engine::general_purpose::STANDARD;

    fn metadata(algorithm: Algorithm, bytes: &[u8]) -> String {
        let (name, digest) = match algorithm {
            Algorithm::Sha256 => ("sha256", Sha256::digest(bytes).to_vec()),
            Algorithm::Sha384 => ("sha384", Sha384::digest(bytes).to_vec()),
            Algorithm::Sha512 => ("sha512", Sha512::digest(bytes).to_vec()),
        };
        format!("{name}-{}", STANDARD.encode(digest))
    }

    #[test]
    fn verifies_raw_bytes_not_decoded_text() {
        let bytes = b"alert('ok')\r\n";
        let value = metadata(Algorithm::Sha384, bytes);
        assert_eq!(verify(&value, bytes, true), Ok(()));
        assert_eq!(
            verify(&value, b"alert('ok')\n", true),
            Err(IntegrityError::DigestMismatch)
        );
    }

    #[test]
    fn strongest_supported_algorithm_wins_and_alternatives_are_or() {
        let bytes = b"body{}";
        let good = metadata(Algorithm::Sha384, bytes);
        let bad = metadata(Algorithm::Sha384, b"wrong");
        let weaker = metadata(Algorithm::Sha256, bytes);
        assert_eq!(
            verify(&format!("{bad} {good} {weaker}"), bytes, true),
            Ok(())
        );
        assert_eq!(
            verify(&format!("{bad} {weaker}"), bytes, true),
            Err(IntegrityError::DigestMismatch)
        );
    }

    #[test]
    fn opaque_response_is_ineligible_even_with_matching_digest() {
        let value = metadata(Algorithm::Sha512, b"script");
        assert_eq!(
            verify(&value, b"script", false),
            Err(IntegrityError::IneligibleResponse)
        );
    }

    #[test]
    fn unknown_or_malformed_metadata_is_ignored() {
        assert_eq!(verify("sha999-abc", b"anything", false), Ok(()));
        assert_eq!(verify("sha256-not-a-digest", b"anything", false), Ok(()));
        let value = metadata(Algorithm::Sha256, b"x")
            .replace('+', "-")
            .replace('/', "_");
        assert_eq!(verify(&value, b"x", true), Ok(()));
    }
}
