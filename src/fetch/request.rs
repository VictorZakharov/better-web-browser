//! Request state and policy-specific constructors.

use super::{
    Body, FetchError, FetchErrorKind, FetchSignal, FetchUrl, HeaderList, Origin, RequestClient,
};
use crate::limits::MAX_RESPONSE_BODY_BYTES;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestContext {
    Navigation,
    Subresource,
    Script,
    /// Internal script loader: may consume no-cors bytes, never exposed as a Fetch Response.
    WorkerScript,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestDestination {
    Document,
    Style,
    Image,
    Script,
    Worker,
    Font,
    Fetch,
    Video,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RequestMode {
    Navigate,
    SameOrigin,
    NoCors,
    Cors,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestCache {
    Default,
    NoStore,
    Reload,
    NoCache,
    ForceCache,
    OnlyIfCached,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ReferrerPolicy {
    NoReferrer,
    NoReferrerWhenDowngrade,
    SameOrigin,
    Origin,
    StrictOrigin,
    OriginWhenCrossOrigin,
    StrictOriginWhenCrossOrigin,
    UnsafeUrl,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Referrer {
    NoReferrer,
    Url(FetchUrl),
}

#[derive(Debug, Clone)]
pub struct FetchRequest {
    pub policy: std::sync::Arc<super::csp::PolicyContainer>,
    /// Renderer client reference, resolved to a browser-owned navigation result.
    pub client: RequestClient,
    /// Nonzero only for a child-document navigation; never an arbitrary origin.
    pub resulting_client: RequestClient,
    /// Embedding document's browser-owned frame policy, distinct from the initiator.
    pub embedding_client: RequestClient,
    pub url: FetchUrl,
    pub method: String,
    pub headers: HeaderList,
    pub body: Option<Body>,
    pub context: RequestContext,
    pub destination: RequestDestination,
    /// Browser-owned CSP input for script elements; never serialized as an HTTP header.
    pub script_source: Option<super::csp::ScriptSource>,
    pub mode: RequestMode,
    /// True only when a navigation is backed by a trusted user gesture.
    pub user_activation: bool,
    pub credentials: CredentialsMode,
    pub cache: RequestCache,
    pub redirect: RedirectMode,
    pub origin: Option<Origin>,
    pub referrer: Referrer,
    pub referrer_policy: ReferrerPolicy,
    pub signal: FetchSignal,
    pub response_body_limit: usize,
}

impl FetchRequest {
    pub fn navigation(url: &str) -> Result<Self, FetchError> {
        Ok(Self {
            policy: Default::default(),
            client: RequestClient::default(),
            resulting_client: RequestClient::default(),
            embedding_client: RequestClient::default(),
            url: FetchUrl::parse(url)?,
            method: "GET".into(),
            headers: HeaderList::new(),
            body: None,
            context: RequestContext::Navigation,
            destination: RequestDestination::Document,
            script_source: None,
            mode: RequestMode::Navigate,
            user_activation: false,
            credentials: CredentialsMode::Include,
            cache: RequestCache::Default,
            redirect: RedirectMode::Follow,
            origin: None,
            referrer: Referrer::NoReferrer,
            referrer_policy: ReferrerPolicy::StrictOriginWhenCrossOrigin,
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
            policy: Default::default(),
            client: RequestClient::default(),
            resulting_client: RequestClient::default(),
            embedding_client: RequestClient::default(),
            url: FetchUrl::parse(url)?,
            method: "GET".into(),
            headers: HeaderList::new(),
            body: None,
            context: RequestContext::Subresource,
            destination,
            script_source: None,
            mode,
            user_activation: false,
            credentials: CredentialsMode::SameOrigin,
            cache: RequestCache::Default,
            redirect: RedirectMode::Follow,
            origin: Some(document.origin()),
            referrer: Referrer::Url(document),
            referrer_policy: ReferrerPolicy::StrictOriginWhenCrossOrigin,
            signal: FetchSignal::default(),
            response_body_limit: MAX_RESPONSE_BODY_BYTES,
        })
    }

    pub fn script(url: &str, document_url: &str) -> Result<Self, FetchError> {
        let document = FetchUrl::parse(document_url)?;
        Ok(Self {
            policy: Default::default(),
            client: RequestClient::default(),
            resulting_client: RequestClient::default(),
            embedding_client: RequestClient::default(),
            url: FetchUrl::parse(url)?,
            method: "GET".into(),
            headers: HeaderList::new(),
            body: None,
            context: RequestContext::Script,
            destination: RequestDestination::Fetch,
            script_source: None,
            mode: RequestMode::Cors,
            user_activation: false,
            credentials: CredentialsMode::SameOrigin,
            cache: RequestCache::Default,
            redirect: RedirectMode::Follow,
            origin: Some(document.origin()),
            referrer: Referrer::Url(document),
            referrer_policy: ReferrerPolicy::StrictOriginWhenCrossOrigin,
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
        let normalized = method.to_ascii_uppercase();
        self.method = if matches!(
            normalized.as_str(),
            "DELETE" | "GET" | "HEAD" | "OPTIONS" | "POST" | "PUT"
        ) {
            normalized
        } else {
            method.to_string()
        };
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
        if matches!(
            self.context,
            RequestContext::Script | RequestContext::WorkerScript
        ) {
            if self.mode == RequestMode::Navigate {
                return Err(FetchError::new(
                    FetchErrorKind::InvalidRequest,
                    "script-initiated requests cannot use navigate mode",
                ));
            }
            self.headers.validate_script_request()?;
            if self.mode == RequestMode::NoCors
                && !matches!(normalized_method.as_str(), "GET" | "HEAD" | "POST")
            {
                return Err(FetchError::new(
                    FetchErrorKind::InvalidRequest,
                    "no-cors requests require a CORS-safelisted method",
                ));
            }
            if self.cache == RequestCache::OnlyIfCached && self.mode != RequestMode::SameOrigin {
                return Err(FetchError::new(
                    FetchErrorKind::InvalidRequest,
                    "only-if-cached requires same-origin mode",
                ));
            }
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
