//! Ordered header-list storage and script-facing header guards.

use super::{FetchError, FetchErrorKind};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Header {
    name: String,
    value: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HeaderList {
    entries: Vec<Header>,
}

impl Header {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn value(&self) -> &str {
        &self.value
    }
}

impl HeaderList {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn append(&mut self, name: &str, value: &str) -> Result<(), FetchError> {
        let name = normalize_name(name)?;
        let value = normalize_value(value)?;
        self.entries.push(Header { name, value });
        Ok(())
    }

    pub fn append_script(&mut self, name: &str, value: &str) -> Result<(), FetchError> {
        let normalized = normalize_name(name)?;
        if is_forbidden_request_header(&normalized) {
            return Err(FetchError::new(
                FetchErrorKind::InvalidRequest,
                format!("script cannot set forbidden request header `{normalized}`"),
            ));
        }
        self.append(&normalized, value)
    }

    pub fn set(&mut self, name: &str, value: &str) -> Result<(), FetchError> {
        let normalized = normalize_name(name)?;
        self.remove(&normalized);
        self.append(&normalized, value)
    }

    pub fn set_script(&mut self, name: &str, value: &str) -> Result<(), FetchError> {
        let normalized = normalize_name(name)?;
        if is_forbidden_request_header(&normalized) {
            return Err(FetchError::new(
                FetchErrorKind::InvalidRequest,
                format!("script cannot set forbidden request header `{normalized}`"),
            ));
        }
        self.remove(&normalized);
        self.append(&normalized, value)
    }

    pub fn get(&self, name: &str) -> Option<&str> {
        self.entries
            .iter()
            .find(|header| header.name.eq_ignore_ascii_case(name))
            .map(Header::value)
    }

    pub fn values<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a str> + 'a {
        self.entries
            .iter()
            .filter(move |header| header.name.eq_ignore_ascii_case(name))
            .map(Header::value)
    }

    pub fn contains(&self, name: &str) -> bool {
        self.get(name).is_some()
    }

    pub fn remove(&mut self, name: &str) {
        self.entries
            .retain(|header| !header.name.eq_ignore_ascii_case(name));
    }

    pub fn iter(&self) -> impl Iterator<Item = &Header> {
        self.entries.iter()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub(crate) fn retain(&mut self, predicate: impl Fn(&Header) -> bool) {
        self.entries.retain(predicate);
    }

    pub(crate) fn to_wire_string(&self) -> String {
        let mut output = String::new();
        for header in &self.entries {
            output.push_str(header.name());
            output.push_str(": ");
            output.push_str(header.value());
            output.push_str("\r\n");
        }
        output
    }

    pub(crate) fn validate_script_request(&self) -> Result<(), FetchError> {
        if let Some(header) = self
            .entries
            .iter()
            .find(|header| is_forbidden_request_header(header.name()))
        {
            return Err(FetchError::new(
                FetchErrorKind::InvalidRequest,
                format!(
                    "script cannot set forbidden request header `{}`",
                    header.name()
                ),
            ));
        }
        Ok(())
    }
}

pub(crate) fn is_forbidden_response_header(name: &str) -> bool {
    name.eq_ignore_ascii_case("set-cookie") || name.eq_ignore_ascii_case("set-cookie2")
}

pub(crate) fn is_cors_safelisted_response_header(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "cache-control"
            | "content-language"
            | "content-length"
            | "content-type"
            | "expires"
            | "last-modified"
            | "pragma"
    )
}

pub(crate) fn is_cors_safelisted_request_header(header: &Header) -> bool {
    let name = header.name();
    let value = header.value().as_bytes();
    if value.len() > 128 {
        return false;
    }
    match name {
        "accept" => !value.iter().any(|byte| is_cors_unsafe_byte(*byte)),
        "accept-language" | "content-language" => value
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || b" *,-.;=".contains(byte)),
        "content-type" => {
            let essence = header
                .value()
                .split(';')
                .next()
                .unwrap_or("")
                .trim()
                .to_ascii_lowercase();
            matches!(
                essence.as_str(),
                "application/x-www-form-urlencoded" | "multipart/form-data" | "text/plain"
            ) && !value.iter().any(|byte| is_cors_unsafe_byte(*byte))
        }
        "range" => is_simple_range_header(header.value()),
        _ => false,
    }
}

fn normalize_name(name: &str) -> Result<String, FetchError> {
    if name.is_empty()
        || !name.bytes().all(|byte| {
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
        })
    {
        return Err(FetchError::new(
            FetchErrorKind::InvalidRequest,
            format!("invalid HTTP header name: {name}"),
        ));
    }
    Ok(name.to_ascii_lowercase())
}

fn normalize_value(value: &str) -> Result<String, FetchError> {
    if value.bytes().any(|byte| matches!(byte, 0 | b'\r' | b'\n')) {
        return Err(FetchError::new(
            FetchErrorKind::InvalidRequest,
            "HTTP header values cannot contain NUL, CR, or LF",
        ));
    }
    Ok(value.trim_matches([' ', '\t']).to_string())
}

pub(crate) fn is_forbidden_request_header(name: &str) -> bool {
    matches!(
        name,
        "accept-charset"
            | "accept-encoding"
            | "access-control-request-headers"
            | "access-control-request-method"
            | "connection"
            | "content-length"
            | "cookie"
            | "cookie2"
            | "date"
            | "dnt"
            | "expect"
            | "host"
            | "keep-alive"
            | "origin"
            | "permissions-policy"
            | "proxy-authenticate"
            | "proxy-authorization"
            | "referer"
            | "set-cookie"
            | "te"
            | "trailer"
            | "transfer-encoding"
            | "upgrade"
            | "via"
    ) || name.starts_with("proxy-")
        || name.starts_with("sec-")
        || matches!(
            name,
            "x-http-method" | "x-http-method-override" | "x-method-override"
        )
}

fn is_cors_unsafe_byte(byte: u8) -> bool {
    byte < 0x20 && byte != b'\t'
        || matches!(
            byte,
            b'"' | b'('
                | b')'
                | b':'
                | b'<'
                | b'>'
                | b'?'
                | b'@'
                | b'['
                | b'\\'
                | b']'
                | b'{'
                | b'}'
        )
        || byte == 0x7f
}

fn is_simple_range_header(value: &str) -> bool {
    let Some(range) = value.strip_prefix("bytes=") else {
        return false;
    };
    let Some((start, end)) = range.split_once('-') else {
        return false;
    };
    let Ok(start) = start.parse::<u64>() else {
        return false;
    };
    end.is_empty() || end.parse::<u64>().is_ok_and(|end| end >= start)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_script_control_of_sensitive_headers() {
        let mut headers = HeaderList::new();
        for name in ["Cookie", "Host", "Origin", "Referer", "Sec-Fetch-Site"] {
            assert!(headers.append_script(name, "injected").is_err(), "{name}");
        }
        assert!(headers.append_script("X-Application", "allowed").is_ok());
    }

    #[test]
    fn rejects_header_line_injection() {
        let mut headers = HeaderList::new();
        assert!(headers.append("x-safe", "yes\r\nx-evil: injected").is_err());
    }
}
