//! CORS checks and response-header filtering from the Fetch standard.

use super::headers::{
    is_cors_safelisted_request_header, is_cors_safelisted_response_header,
    is_forbidden_response_header,
};
use super::{CredentialsMode, FetchError, FetchErrorKind, FetchRequest, HeaderList, RequestMode};
use std::collections::HashSet;

pub(crate) fn needs_cors_check(request: &FetchRequest) -> bool {
    request
        .origin
        .as_ref()
        .is_some_and(|origin| !origin.is_same_origin(&request.url.origin()))
}

pub(crate) fn needs_preflight(request: &FetchRequest) -> bool {
    needs_cors_check(request)
        && request.mode == RequestMode::Cors
        && (!matches!(request.method.as_str(), "GET" | "HEAD" | "POST")
            || request
                .headers
                .iter()
                .any(|header| !is_cors_safelisted_request_header(header)))
}

pub(crate) fn validate_cors_response(
    request: &FetchRequest,
    headers: &HeaderList,
) -> Result<(), FetchError> {
    if !needs_cors_check(request) {
        return Ok(());
    }
    match request.mode {
        RequestMode::SameOrigin => Err(cors_error(
            "cross-origin request blocked by same-origin mode",
        )),
        RequestMode::Cors => validate_allow_origin(request, headers),
        RequestMode::Navigate | RequestMode::NoCors => Ok(()),
    }
}

pub(crate) fn validate_preflight_response(
    request: &FetchRequest,
    headers: &HeaderList,
) -> Result<(), FetchError> {
    validate_allow_origin(request, headers)?;
    if !matches!(request.method.as_str(), "GET" | "HEAD" | "POST") {
        let allowed_methods = token_list(headers, "access-control-allow-methods");
        let method_allowed = allowed_methods
            .iter()
            .any(|method| method.eq_ignore_ascii_case(&request.method))
            || request.credentials != CredentialsMode::Include
                && allowed_methods.iter().any(|method| method == "*");
        if !method_allowed {
            return Err(cors_error(format!(
                "CORS preflight did not allow method {}",
                request.method
            )));
        }
    }

    let allowed_headers = token_list(headers, "access-control-allow-headers");
    let wildcard = allowed_headers.iter().any(|header| header == "*")
        && request.credentials != CredentialsMode::Include;
    for header in request
        .headers
        .iter()
        .filter(|header| !is_cors_safelisted_request_header(header))
    {
        if !wildcard
            && !allowed_headers
                .iter()
                .any(|allowed| allowed.eq_ignore_ascii_case(header.name()))
        {
            return Err(cors_error(format!(
                "CORS preflight did not allow header {}",
                header.name()
            )));
        }
    }
    Ok(())
}

pub(crate) fn cors_filtered_headers(headers: &HeaderList, request: &FetchRequest) -> HeaderList {
    let mut filtered = headers.clone();
    let cors = needs_cors_check(request) && request.mode == RequestMode::Cors;
    let exposed = if cors {
        exposed_header_names(headers, request.credentials)
    } else {
        HashSet::new()
    };
    filtered.retain(|header| {
        !is_forbidden_response_header(header.name())
            && (!cors
                || exposed.contains("*")
                || is_cors_safelisted_response_header(header.name())
                || exposed.contains(header.name()))
    });
    filtered
}

fn validate_allow_origin(request: &FetchRequest, headers: &HeaderList) -> Result<(), FetchError> {
    let origin = request
        .origin
        .as_ref()
        .expect("cross-origin requests have a client origin")
        .serialize();
    let allowed_origins = headers
        .values("access-control-allow-origin")
        .collect::<Vec<_>>();
    if allowed_origins.len() != 1 {
        return Err(cors_error(
            "CORS response must contain exactly one Access-Control-Allow-Origin value",
        ));
    }
    let allowed = allowed_origins[0];
    let wildcard_allowed = allowed == "*" && request.credentials != CredentialsMode::Include;
    if !wildcard_allowed && allowed != origin {
        return Err(cors_error(format!(
            "CORS response allowed `{allowed}` instead of `{origin}`"
        )));
    }
    if request.credentials == CredentialsMode::Include {
        let credentials = headers
            .values("access-control-allow-credentials")
            .collect::<Vec<_>>();
        if credentials.as_slice() != ["true"] {
            return Err(cors_error(
                "credentialed CORS response requires one Access-Control-Allow-Credentials: true",
            ));
        }
    }
    Ok(())
}

fn token_list(headers: &HeaderList, name: &str) -> Vec<String> {
    headers
        .values(name)
        .flat_map(|value| value.split(','))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_ascii_lowercase())
        .collect()
}

fn exposed_header_names(headers: &HeaderList, credentials: CredentialsMode) -> HashSet<String> {
    let mut names = token_list(headers, "access-control-expose-headers")
        .into_iter()
        .collect::<HashSet<_>>();
    if credentials == CredentialsMode::Include {
        names.remove("*");
    }
    names
}

fn cors_error(message: impl Into<String>) -> FetchError {
    FetchError::new(FetchErrorKind::Cors, message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fetch::FetchRequest;

    #[test]
    fn validates_cross_origin_headers_and_filters_private_metadata() {
        let request =
            FetchRequest::script("https://api.example/data", "https://app.example/index.html")
                .unwrap();
        let mut headers = HeaderList::new();
        headers
            .append("access-control-allow-origin", "https://app.example")
            .unwrap();
        headers.append("content-type", "application/json").unwrap();
        headers.append("x-public", "yes").unwrap();
        headers.append("x-private", "no").unwrap();
        headers
            .append("access-control-expose-headers", "X-Public")
            .unwrap();
        headers.append("set-cookie", "secret=value").unwrap();

        validate_cors_response(&request, &headers).unwrap();
        let filtered = cors_filtered_headers(&headers, &request);
        assert_eq!(filtered.get("x-public"), Some("yes"));
        assert_eq!(filtered.get("x-private"), None);
        assert_eq!(filtered.get("set-cookie"), None);
    }
}
