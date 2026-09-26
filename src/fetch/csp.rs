//! Enforced source-list policy containers for child documents.
//! https://www.w3.org/TR/CSP3/#framework-directives
//! Known unsupported enforcement directives fail closed. Unknown directives and
//! unmatched source expressions follow CSP3's forward-compatible parsing rules.
//! Inline hash admission currently covers script elements. Style-element hash
//! admission and `'unsafe-hashes'` for attributes require separate lifecycle hooks.
mod inline;
mod sources;
#[cfg(test)]
mod tests;
use super::{FetchError, FetchErrorKind, HeaderList, RequestDestination};
pub use inline::InlineScriptViolation;
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

fn parse_policy(url: &str, serialized: &str, from_meta: bool) -> Result<Policy, FetchError> {
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
        if from_meta && matches!(name.as_str(), "report-uri" | "frame-ancestors" | "sandbox") {
            continue;
        }
        let mut values: Vec<_> = tokens.map(str::to_owned).collect();
        if name == "block-all-mixed-content" {
            policy.mixed_content = true;
        } else if matches!(name.as_str(), "report-uri" | "report-to") {
            // Reporting does not affect enforcement. Until reporting is implemented,
            // preserve the remaining directives in this policy.
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
            if matches!(
                name.as_str(),
                "sandbox"
                    | "upgrade-insecure-requests"
                    | "require-trusted-types-for"
                    | "trusted-types"
                    | "navigate-to"
                    | "webrtc"
            ) {
                return Err(unsupported(&name));
            }
            // CSP3 §2.2.1 ignores unknown directive names for forward compatibility.
            continue;
        } else {
            // Matching ignores invalid or unsupported source expressions individually.
            // An empty remaining list denies the directive rather than relaxing it.
            values.retain(|value| sources::supported(value));
        }
        policy.directives.insert(name, values);
    }
    Ok(policy)
}

impl PolicyContainer {
    pub fn from_serialized_policies(url: &str, policies: &[String]) -> Result<Self, FetchError> {
        if policies.len() > 32 {
            return Err(unsupported("policy count"));
        }
        Ok(Self {
            policies: policies
                .iter()
                .map(|policy| parse_policy(url, policy, false))
                .collect::<Result<Vec<_>, _>>()?,
        })
    }

    pub fn from_headers(url: &str, headers: &HeaderList) -> Result<Self, FetchError> {
        let mut result = Self::default();
        for header in headers.values("content-security-policy") {
            for serialized in header.split(',') {
                if result.policies.len() >= 32 {
                    return Err(unsupported("policy count"));
                }
                result.policies.push(parse_policy(url, serialized, false)?);
            }
        }
        Ok(result)
    }

    /// Meta policies start at insertion time and restrict, rather than replace, response policies.
    /// HTML ignores these three directives for a meta-delivered policy.
    /// https://html.spec.whatwg.org/multipage/semantics.html#attr-meta-http-equiv-content-security-policy
    pub fn append_meta(&mut self, url: &str, serialized: &str) -> Result<(), FetchError> {
        if self.policies.len() >= 32 {
            return Err(unsupported("policy count"));
        }
        self.policies.push(parse_policy(url, serialized, true)?);
        Ok(())
    }

    pub fn serialized_policies(&self) -> impl Iterator<Item = &str> {
        self.policies
            .iter()
            .map(|policy| policy.serialized.as_str())
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

    /// CSP3 §6.7.2.2 admits a resource hint when *any explicit fetch source list*
    /// matches, but only if this policy contains `default-src` at all. Each
    /// policy in the container is still checked independently.
    /// https://www.w3.org/TR/CSP3/#does-resource-hint-request-violate-policy
    pub fn allows_resource_hint(&self, url: &str) -> bool {
        let Ok(url) = url::Url::parse(url) else {
            return false;
        };
        const FETCH_DIRECTIVES: &[&str] = &[
            "child-src",
            "connect-src",
            "font-src",
            "frame-src",
            "img-src",
            "manifest-src",
            "media-src",
            "object-src",
            "script-src",
            "script-src-elem",
            "style-src",
            "style-src-elem",
            "worker-src",
        ];
        self.policies.iter().all(|policy| {
            if policy.mixed_content && policy.origin.scheme() == "https" && url.scheme() == "http" {
                return false;
            }
            if !policy.directives.contains_key("default-src") {
                return true;
            }
            FETCH_DIRECTIVES.iter().any(|name| {
                policy.directives.get(*name).is_some_and(|list| {
                    list.iter()
                        .any(|source| sources::matches(source, &url, &policy.origin, 0))
                })
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
        self.policies
            .iter()
            .all(|policy| policy_allows_script_url(policy, directive, &url, redirects, script))
    }

    /// Return each enforcing policy that rejected a script URL. The caller owns
    /// event queuing; pure admission must not dispatch events during Fetch retries.
    pub fn script_url_violations(
        &self,
        directive: &str,
        url: &str,
        redirects: usize,
        script: &ScriptSource,
    ) -> Vec<UrlViolation<'_>> {
        let Ok(url) = url::Url::parse(url) else {
            return Vec::new();
        };
        self.policies
            .iter()
            .filter(|policy| !policy_allows_script_url(policy, directive, &url, redirects, script))
            .map(|policy| UrlViolation {
                original_policy: &policy.serialized,
            })
            .collect()
    }

    pub fn allows_eval(&self) -> bool {
        self.allows_keyword("script-src", "'unsafe-eval'")
    }
    pub fn allows_style_inline(&self) -> bool {
        // Style hash sources are recognized but are not admitted until style
        // processing can preserve policy timing and report violations.
        self.policies.iter().all(|policy| {
            policy.list("style-src-elem").is_none_or(|list| {
                !list.iter().any(|source| {
                    sources::nonce_value(source).is_some() || sources::hash_source(source)
                }) && has_keyword(list, "'unsafe-inline'")
            })
        })
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
            RequestDestination::Audio => "media-src",
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
pub struct UrlViolation<'a> {
    pub original_policy: &'a str,
}

fn policy_allows_script_url(
    policy: &Policy,
    directive: &str,
    url: &url::Url,
    redirects: usize,
    script: &ScriptSource,
) -> bool {
    if policy.mixed_content && policy.origin.scheme() == "https" && url.scheme() == "http" {
        return false;
    }
    policy.list(directive).is_none_or(|list| {
        if nonce_matches(list, script.nonce.as_deref()) {
            return true;
        }
        // A trusted non-parser-inserted script may inherit strict-dynamic's
        // authority; host/scheme expressions do not regain authority here.
        if has_keyword(list, "'strict-dynamic'") {
            return !script.parser_inserted;
        }
        list.iter()
            .any(|source| sources::matches(source, url, &policy.origin, redirects))
    })
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
