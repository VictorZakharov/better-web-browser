//! Fetch URL and tuple-origin types backed by the WHATWG-oriented `url` crate.

use super::{FetchError, FetchErrorKind};
use crate::navigation::{ParsedUrl, resolve_url};
use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_OPAQUE_ORIGIN: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Origin {
    kind: OriginKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum OriginKind {
    Tuple {
        scheme: String,
        host: String,
        port: u16,
    },
    Opaque(u64),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FetchUrl {
    serialized: String,
    parsed: ParsedUrl,
}

impl FetchUrl {
    pub fn parse(input: &str) -> Result<Self, FetchError> {
        let parsed = ParsedUrl::parse(input)
            .map_err(|error| FetchError::new(FetchErrorKind::InvalidRequest, error.to_string()))?;
        Ok(Self {
            serialized: parsed.canonical(),
            parsed,
        })
    }

    pub fn resolve(&self, reference: &str) -> Result<Self, FetchError> {
        let resolved = resolve_url(self.as_str(), reference).ok_or_else(|| {
            FetchError::new(
                FetchErrorKind::InvalidRequest,
                format!("could not resolve URL reference: {reference}"),
            )
        })?;
        Self::parse(&resolved)
    }

    pub fn as_str(&self) -> &str {
        &self.serialized
    }

    pub fn parsed(&self) -> &ParsedUrl {
        &self.parsed
    }

    pub fn origin(&self) -> Origin {
        Origin {
            kind: OriginKind::Tuple {
                scheme: self.parsed.scheme.clone(),
                host: self.parsed.host.to_ascii_lowercase(),
                port: self.parsed.port,
            },
        }
    }

    pub fn is_secure(&self) -> bool {
        self.parsed.scheme == "https"
    }
}

impl Origin {
    pub fn parse(url: &str) -> Result<Self, FetchError> {
        Ok(FetchUrl::parse(url)?.origin())
    }

    pub fn serialize(&self) -> String {
        match &self.kind {
            OriginKind::Tuple { scheme, host, port } => ParsedUrl {
                scheme: scheme.clone(),
                host: host.clone(),
                port: *port,
                path_and_query: "/".into(),
            }
            .origin(),
            OriginKind::Opaque(_) => "null".into(),
        }
    }

    pub fn is_same_origin(&self, other: &Self) -> bool {
        self == other
    }

    pub fn opaque() -> Self {
        Self {
            kind: OriginKind::Opaque(NEXT_OPAQUE_ORIGIN.fetch_add(1, Ordering::Relaxed)),
        }
    }
}

impl fmt::Display for FetchUrl {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl fmt::Display for Origin {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.serialize())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_ports_share_an_origin() {
        let explicit = Origin::parse("https://example.com:443/path").unwrap();
        let implicit = Origin::parse("https://EXAMPLE.com/other").unwrap();
        assert!(explicit.is_same_origin(&implicit));
        assert_eq!(explicit.serialize(), "https://example.com");
    }

    #[test]
    fn opaque_origins_only_match_their_own_clones() {
        let first = Origin::opaque();
        let clone = first.clone();
        let second = Origin::opaque();
        assert!(first.is_same_origin(&clone));
        assert!(!first.is_same_origin(&second));
        assert_eq!(first.serialize(), "null");
    }
}
