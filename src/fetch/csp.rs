//! Enforced source-list policy containers for child documents.
//! https://www.w3.org/TR/CSP3/#framework-directives
//! Unsupported policy features fail closed at admission; they are not ignored.
mod sources;
#[cfg(test)]
mod tests;
use super::{FetchError, FetchErrorKind, HeaderList, RequestDestination};
use std::collections::HashMap;

#[derive(Debug, Clone, Default)]
pub struct PolicyContainer {
    policies: Vec<Policy>,
}

/// Element metadata needed by CSP3 before a script request is sent or executed.
/// Absence of metadata is treated as parser-inserted with no nonce (fail closed).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ScriptSource {
    pub nonce: Option<String>,
    pub parser_inserted: bool,
}

impl Default for ScriptSource {
    fn default() -> Self {
        Self {
            nonce: None,
            parser_inserted: true,
        }
    }
}

#[derive(Debug, Clone)]
struct Policy {
    origin: url::Url,
    serialized: String,
    directives: HashMap<String, Vec<String>>,
    mixed_content: bool,
}

impl PolicyContainer {
    pub fn from_headers(url: &str, headers: &HeaderList) -> Result<Self, FetchError> {
        let mut result = Self::default();
        for header in headers.values("content-security-policy") {
            for serialized in header.split(',') {
                if result.policies.len() >= 32 {
                    return Err(unsupported("policy count"));
                }
                let mut policy = Policy {
                    origin: url::Url::parse(url).map_err(|_| unsupported("origin URL"))?,
                    serialized: serialized.trim().to_owned(),
                    directives: HashMap::new(),
                    mixed_content: false,
                };
                for directive in serialized.split(';') {
                    let mut tokens = directive.split_ascii_whitespace();
                    let Some(name) = tokens.next().map(str::to_ascii_lowercase) else {
                        continue;
                    };
                    if policy.directives.contains_key(&name) {
                        continue;
                    }
                    let values: Vec<_> = tokens.map(str::to_owned).collect();
                    if name == "block-all-mixed-content" {
                        policy.mixed_content = true;
                    } else if name == "report-uri" {
                        // Reporting does not affect enforcement. Until reporting is
                        // implemented, preserve the remaining directives in this policy.
                        continue;
                    } else if !matches!(
                        name.as_str(),
                        "default-src"
                            | "script-src"
                            | "script-src-elem"
                            | "script-src-attr"
                            | "style-src"
                            | "style-src-elem"
                            | "style-src-attr"
                            | "connect-src"
                            | "img-src"
                            | "font-src"
                            | "media-src"
                            | "manifest-src"
                            | "object-src"
                            | "child-src"
                            | "frame-src"
                            | "worker-src"
                            | "base-uri"
                            | "form-action"
                            | "frame-ancestors"
                    ) {
                        return Err(unsupported(&name));
                    } else if values.iter().any(|value| !sources::supported(value)) {
                        return Err(unsupported(&format!("{name} source expression")));
                    }
                    policy.directives.insert(name, values);
                }
                result.policies.push(policy);
            }
        }
        Ok(result)
    }

    pub fn is_empty(&self) -> bool {
        self.policies.is_empty()
    }

    pub fn allows_url(&self, directive: &str, url: &str, redirects: usize) -> bool {
        if matches!(directive, "script-src" | "script-src-elem") {
            return self.allows_script_url(directive, url, redirects, &ScriptSource::default());
        }
        let Ok(url) = url::Url::parse(url) else {
            return false;
        };
        self.policies.iter().all(|policy| {
            !(policy.mixed_content && policy.origin.scheme() == "https" && url.scheme() == "http")
                && policy.list(directive).is_none_or(|list| {
                    list.iter()
                        .any(|source| sources::matches(source, &url, &policy.origin, redirects))
                })
        })
    }

    pub fn allows_script_url(
        &self,
        directive: &str,
        url: &str,
        redirects: usize,
        script: &ScriptSource,
    ) -> bool {
        let Ok(url) = url::Url::parse(url) else {
            return false;
        };
        self.policies.iter().all(|policy| {
            if policy.mixed_content && policy.origin.scheme() == "https" && url.scheme() == "http" {
                return false;
            }
            policy.list(directive).is_none_or(|list| {
                if nonce_matches(list, script.nonce.as_deref()) {
                    return true;
                }
                // CSP3 ignores host/scheme sources under strict-dynamic. Only a
                // non-parser-inserted script can inherit the trusted loader's authority.
                if has_keyword(list, "'strict-dynamic'") {
                    return !script.parser_inserted;
                }
                list.iter()
                    .any(|source| sources::matches(source, &url, &policy.origin, redirects))
            })
        })
    }

    pub fn allows_inline(&self, attribute: bool) -> bool {
        self.allows_inline_with_nonce(attribute, None)
    }

    pub fn allows_inline_with_nonce(&self, attribute: bool, nonce: Option<&str>) -> bool {
        let directive = if attribute {
            "script-src-attr"
        } else {
            "script-src-elem"
        };
        self.policies.iter().all(|policy| {
            policy.list(directive).is_none_or(|list| {
                if !attribute && nonce_matches(list, nonce) {
                    return true;
                }
                if has_keyword(list, "'strict-dynamic'")
                    || list.iter().any(|source| {
                        sources::nonce_value(source).is_some() || sources::hash_source(source)
                    })
                {
                    return false;
                }
                has_keyword(list, "'unsafe-inline'")
            })
        })
    }

    /// Each enforcing policy that rejects an inline script reports separately.
    /// Keep the original serialization for `SecurityPolicyViolationEvent`, not a
    /// reconstructed directive map which could change casing or source order.
    pub fn inline_script_violations(&self, nonce: Option<&str>) -> Vec<InlineScriptViolation<'_>> {
        self.policies
            .iter()
            .filter_map(|policy| {
                let list = policy.list("script-src-elem")?;
                if nonce_matches(list, nonce) {
                    return None;
                }
                let restricted = has_keyword(list, "'strict-dynamic'")
                    || list.iter().any(|source| {
                        sources::nonce_value(source).is_some() || sources::hash_source(source)
                    });
                if !restricted && has_keyword(list, "'unsafe-inline'") {
                    return None;
                }
                Some(InlineScriptViolation {
                    original_policy: &policy.serialized,
                    report_sample: has_keyword(list, "'report-sample'"),
                })
            })
            .collect()
    }

    pub fn allows_eval(&self) -> bool {
        self.allows_keyword("script-src", "'unsafe-eval'")
    }
    pub fn allows_style_inline(&self) -> bool {
        self.allows_keyword("style-src-elem", "'unsafe-inline'")
    }

    fn allows_keyword(&self, directive: &str, keyword: &str) -> bool {
        self.policies.iter().all(|policy| {
            policy.list(directive).is_none_or(|list| {
                list.iter()
                    .any(|source| source.eq_ignore_ascii_case(keyword))
            })
        })
    }

    pub fn checks_ancestors(&self) -> bool {
        self.policies
            .iter()
            .any(|policy| policy.directives.contains_key("frame-ancestors"))
    }

    pub fn check_request(
        &self,
        destination: RequestDestination,
        url: &str,
        redirects: usize,
    ) -> Result<(), FetchError> {
        self.check_request_with_script(destination, url, redirects, None)
    }

    pub fn check_request_with_script(
        &self,
        destination: RequestDestination,
        url: &str,
        redirects: usize,
        script: Option<&ScriptSource>,
    ) -> Result<(), FetchError> {
        let directive = match destination {
            RequestDestination::Document => "frame-src",
            RequestDestination::Script => "script-src-elem",
            RequestDestination::Worker => "worker-src",
            RequestDestination::Fetch => "connect-src",
            RequestDestination::Style => "style-src-elem",
            RequestDestination::Image => "img-src",
            RequestDestination::Font => "font-src",
            RequestDestination::Video => "media-src",
        };
        let allowed = if destination == RequestDestination::Script {
            self.allows_script_url(
                directive,
                url,
                redirects,
                script.unwrap_or(&ScriptSource::default()),
            )
        } else {
            self.allows_url(directive, url, redirects)
        };
        if allowed {
            Ok(())
        } else {
            Err(FetchError::new(
                FetchErrorKind::Network,
                format!("Content Security Policy blocked {directive} request"),
            ))
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InlineScriptViolation<'a> {
    pub original_policy: &'a str,
    pub report_sample: bool,
}

fn has_keyword(list: &[String], keyword: &str) -> bool {
    list.iter()
        .any(|source| source.eq_ignore_ascii_case(keyword))
}

fn nonce_matches(list: &[String], nonce: Option<&str>) -> bool {
    nonce.is_some_and(|nonce| {
        !nonce.is_empty()
            && list
                .iter()
                .any(|source| sources::nonce_value(source) == Some(nonce))
    })
}

impl Policy {
    fn list(&self, name: &str) -> Option<&Vec<String>> {
        let names: &[&str] = match name {
            "script-src-elem" => &["script-src-elem", "script-src", "default-src"],
            "script-src-attr" => &["script-src-attr", "script-src", "default-src"],
            "style-src-elem" => &["style-src-elem", "style-src", "default-src"],
            "style-src-attr" => &["style-src-attr", "style-src", "default-src"],
            "frame-src" => &["frame-src", "child-src", "default-src"],
            "worker-src" => &["worker-src", "child-src", "script-src", "default-src"],
            "base-uri" | "form-action" | "frame-ancestors" => &[name],
            _ => &[name, "default-src"],
        };
        names.iter().find_map(|name| self.directives.get(*name))
    }
}

fn unsupported(feature: &str) -> FetchError {
    FetchError::new(
        FetchErrorKind::Network,
        format!("Content Security Policy requires unsupported {feature}; child document refused"),
    )
}
