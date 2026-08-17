//! Fetch URL and tuple-origin types backed by the WHATWG-oriented `url` crate.

use super::{FetchError, FetchErrorKind};
use crate::limits::MAX_URL_BYTES;
use crate::navigation::{ParsedUrl, resolve_web_url};
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
    kind: FetchUrlKind,
    origin: Origin,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum FetchUrlKind {
    Network(ParsedUrl),
    Data,
}

impl FetchUrl {
    pub fn parse(input: &str) -> Result<Self, FetchError> {
        if input.len() > MAX_URL_BYTES {
            return Err(FetchError::new(
                FetchErrorKind::InvalidRequest,
                format!("URL exceeds the {MAX_URL_BYTES}-byte limit"),
            ));
        }
        let input = input.trim();
        let parsed_url = ::url::Url::parse(input)
            .map_err(|error| FetchError::new(FetchErrorKind::InvalidRequest, error.to_string()))?;
        match parsed_url.scheme() {
            "http" | "https" => {
                let parsed = ParsedUrl::parse(input).map_err(|error| {
                    FetchError::new(FetchErrorKind::InvalidRequest, error.to_string())
                })?;
                let origin = tuple_origin(&parsed);
                Ok(Self {
                    serialized: parsed.canonical(),
                    kind: FetchUrlKind::Network(parsed),
                    origin,
                })
            }
            "data" => {
                let mut parsed_url = parsed_url;
                parsed_url.set_fragment(None);
                let serialized = parsed_url.to_string();
                if serialized.len() > MAX_URL_BYTES {
                    return Err(FetchError::new(
                        FetchErrorKind::InvalidRequest,
                        format!("canonical URL exceeds the {MAX_URL_BYTES}-byte limit"),
                    ));
                }
                Ok(Self {
                    serialized,
                    kind: FetchUrlKind::Data,
                    origin: Origin::opaque(),
                })
            }
            scheme => Err(FetchError::new(
                FetchErrorKind::InvalidRequest,
                format!("unsupported Fetch URL scheme: {scheme}"),
            )),
        }
    }

    pub fn resolve(&self, reference: &str) -> Result<Self, FetchError> {
        let resolved = resolve_web_url(self.as_str(), reference).ok_or_else(|| {
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

    pub fn parsed(&self) -> Option<&ParsedUrl> {
        match &self.kind {
            FetchUrlKind::Network(parsed) => Some(parsed),
            FetchUrlKind::Data => None,
        }
    }

    pub fn is_data(&self) -> bool {
        matches!(self.kind, FetchUrlKind::Data)
    }

    pub fn origin(&self) -> Origin {
        self.origin.clone()
    }

    pub fn is_secure(&self) -> bool {
        self.parsed().is_some_and(|parsed| parsed.scheme == "https")
    }
}

fn tuple_origin(parsed: &ParsedUrl) -> Origin {
    Origin {
        kind: OriginKind::Tuple {
            scheme: parsed.scheme.clone(),
            host: parsed.host.to_ascii_lowercase(),
            port: parsed.port,
        },
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

    pub fn is_secure(&self) -> bool {
        matches!(&self.kind, OriginKind::Tuple { scheme, .. } if scheme == "https")
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

    #[test]
    fn data_urls_have_stable_opaque_origins_and_drop_fragments() {
        let url = FetchUrl::parse("data:text/plain,hello#fragment").unwrap();
        let clone = url.clone();
        assert!(url.is_data());
        assert!(url.parsed().is_none());
        assert_eq!(url.as_str(), "data:text/plain,hello");
        assert!(url.origin().is_same_origin(&clone.origin()));
        assert!(
            !url.origin()
                .is_same_origin(&FetchUrl::parse("data:text/plain,hello").unwrap().origin())
        );
    }
}
