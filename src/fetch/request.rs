//! Request state and policy-specific constructors.

use super::{Body, FetchError, FetchErrorKind, FetchSignal, FetchUrl, HeaderList, Origin};
use crate::limits::MAX_RESPONSE_BODY_BYTES;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestContext {
    Navigation,
    Subresource,
    Script,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestDestination {
    Document,
    Style,
    Image,
    Script,
    Font,
    Fetch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestMode {
    Navigate,
    SameOrigin,
    NoCors,
    Cors,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CredentialsMode {
    Omit,
    SameOrigin,
    Include,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RedirectMode {
    Follow,
    Error,
    Manual,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Referrer {
    NoReferrer,
    Url(FetchUrl),
}

#[derive(Debug, Clone)]
pub struct FetchRequest {
    pub url: FetchUrl,
    pub method: String,
    pub headers: HeaderList,
    pub body: Option<Body>,
    pub context: RequestContext,
    pub destination: RequestDestination,
    pub mode: RequestMode,
    pub credentials: CredentialsMode,
    pub redirect: RedirectMode,
    pub origin: Option<Origin>,
    pub referrer: Referrer,
    pub signal: FetchSignal,
    pub response_body_limit: usize,
}

impl FetchRequest {
    pub fn navigation(url: &str) -> Result<Self, FetchError> {
        Ok(Self {
            url: FetchUrl::parse(url)?,
            method: "GET".into(),
            headers: HeaderList::new(),
            body: None,
            context: RequestContext::Navigation,
            destination: RequestDestination::Document,
            mode: RequestMode::Navigate,
            credentials: CredentialsMode::Include,
            redirect: RedirectMode::Follow,
            origin: None,
            referrer: Referrer::NoReferrer,
            signal: FetchSignal::default(),
            response_body_limit: MAX_RESPONSE_BODY_BYTES,
        })
    }

    pub fn subresource(
        url: &str,
        document_url: &str,
        destination: RequestDestination,
    ) -> Result<Self, FetchError> {
        let document = FetchUrl::parse(document_url)?;
        let mode = if destination == RequestDestination::Font {
            RequestMode::Cors
        } else {
            RequestMode::NoCors
        };
        Ok(Self {
            url: FetchUrl::parse(url)?,
            method: "GET".into(),
            headers: HeaderList::new(),
            body: None,
            context: RequestContext::Subresource,
            destination,
            mode,
            credentials: CredentialsMode::SameOrigin,
            redirect: RedirectMode::Follow,
            origin: Some(document.origin()),
            referrer: Referrer::Url(document),
            signal: FetchSignal::default(),
            response_body_limit: MAX_RESPONSE_BODY_BYTES,
        })
    }

    pub fn script(url: &str, document_url: &str) -> Result<Self, FetchError> {
        let document = FetchUrl::parse(document_url)?;
        Ok(Self {
            url: FetchUrl::parse(url)?,
            method: "GET".into(),
            headers: HeaderList::new(),
            body: None,
            context: RequestContext::Script,
            destination: RequestDestination::Fetch,
            mode: RequestMode::Cors,
            credentials: CredentialsMode::SameOrigin,
            redirect: RedirectMode::Follow,
            origin: Some(document.origin()),
            referrer: Referrer::Url(document),
            signal: FetchSignal::default(),
            response_body_limit: MAX_RESPONSE_BODY_BYTES,
        })
    }

    pub fn set_method(&mut self, method: &str) -> Result<(), FetchError> {
        if method.is_empty() || !method.bytes().all(is_method_byte) {
            return Err(FetchError::new(
                FetchErrorKind::InvalidRequest,
                format!("invalid HTTP method: {method}"),
            ));
        }
        if matches!(
            method.to_ascii_uppercase().as_str(),
            "CONNECT" | "TRACE" | "TRACK"
        ) {
            return Err(FetchError::new(
                FetchErrorKind::InvalidRequest,
                format!("forbidden HTTP method: {method}"),
            ));
        }
        self.method = method.to_ascii_uppercase();
        Ok(())
    }

    pub fn set_script_header(&mut self, name: &str, value: &str) -> Result<(), FetchError> {
        if self.context != RequestContext::Script {
            return Err(FetchError::new(
                FetchErrorKind::InvalidRequest,
                "script header guard requires a script-initiated request",
            ));
        }
        self.headers.set_script(name, value)
    }

    pub fn with_signal(mut self, signal: FetchSignal) -> Self {
        self.signal = signal;
        self
    }

    pub fn with_response_body_limit(mut self, limit: usize) -> Self {
        self.response_body_limit = limit;
        self
    }

    pub(crate) fn validate(&mut self) -> Result<(), FetchError> {
        if self.method.is_empty() || !self.method.bytes().all(is_method_byte) {
            return Err(FetchError::new(
                FetchErrorKind::InvalidRequest,
                format!("invalid HTTP method: {}", self.method),
            ));
        }
        let normalized_method = self.method.to_ascii_uppercase();
        if matches!(
            normalized_method.as_str(),
            "DELETE" | "GET" | "HEAD" | "OPTIONS" | "POST" | "PUT"
        ) {
            self.method = normalized_method.clone();
        }
        if matches!(normalized_method.as_str(), "CONNECT" | "TRACE" | "TRACK") {
            return Err(FetchError::new(
                FetchErrorKind::InvalidRequest,
                format!("forbidden HTTP method: {}", self.method),
            ));
        }
        if matches!(normalized_method.as_str(), "GET" | "HEAD") && self.body.is_some() {
            return Err(FetchError::new(
                FetchErrorKind::InvalidRequest,
                format!("{} requests cannot have a body", self.method),
            ));
        }
        if self.context == RequestContext::Script {
            if self.mode == RequestMode::Navigate {
                return Err(FetchError::new(
                    FetchErrorKind::InvalidRequest,
                    "script-initiated requests cannot use navigate mode",
                ));
            }
            self.headers.validate_script_request()?;
        }
        Ok(())
    }
}

fn is_method_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric()
        || matches!(
            byte,
            b'!' | b'#'
                | b'$'
                | b'%'
                | b'&'
                | b'\''
                | b'*'
                | b'+'
                | b'-'
                | b'.'
                | b'^'
                | b'_'
                | b'`'
                | b'|'
                | b'~'
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constructors_keep_navigation_subresource_and_script_policies_distinct() {
        let navigation = FetchRequest::navigation("https://example.com/").unwrap();
        assert_eq!(navigation.context, RequestContext::Navigation);
        assert_eq!(navigation.mode, RequestMode::Navigate);
        assert_eq!(navigation.credentials, CredentialsMode::Include);

        let image = FetchRequest::subresource(
            "https://cdn.example/image.png",
            "https://example.com/",
            RequestDestination::Image,
        )
        .unwrap();
        assert_eq!(image.context, RequestContext::Subresource);
        assert_eq!(image.mode, RequestMode::NoCors);
        assert_eq!(image.credentials, CredentialsMode::SameOrigin);

        let font = FetchRequest::subresource(
            "https://cdn.example/font.woff2",
            "https://example.com/",
            RequestDestination::Font,
        )
        .unwrap();
        assert_eq!(font.mode, RequestMode::Cors);

        let script =
            FetchRequest::script("https://api.example/data", "https://example.com/").unwrap();
        assert_eq!(script.context, RequestContext::Script);
        assert_eq!(script.mode, RequestMode::Cors);
    }
}
