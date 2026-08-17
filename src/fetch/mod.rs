//! Standards-facing request and response model shared by browser networking consumers.
//!
//! The model follows the WHATWG Fetch and URL concepts. Platform transports remain responsible
//! for DNS, proxying, HTTP framing, TLS, and certificate verification.

mod body;
mod cancellation;
mod cors;
mod error;
mod headers;
mod request;
mod response;
mod url;

pub use body::Body;
pub use cancellation::{FetchController, FetchSignal};
pub use error::{FetchError, FetchErrorKind};
pub use headers::{Header, HeaderList};
pub use request::{
    CredentialsMode, FetchRequest, RedirectMode, Referrer, ReferrerPolicy, RequestCache,
    RequestContext, RequestDestination, RequestMode,
};
pub use response::{FetchResponse, ResponseType};
pub use url::{FetchUrl, Origin};

pub(crate) use cors::{
    cors_filtered_headers, needs_cors_check, needs_preflight, validate_cors_response,
    validate_preflight_response,
};
pub(crate) use headers::is_cors_safelisted_request_header;
