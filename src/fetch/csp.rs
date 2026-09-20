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

#[derive(Debug, Clone)]
struct Policy {
    origin: url::Url,
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

    pub fn allows_inline(&self, attribute: bool) -> bool {
        self.allows_keyword(
            if attribute {
                "script-src-attr"
            } else {
                "script-src-elem"
            },
            "'unsafe-inline'",
        )
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
        let directive = match destination {
            RequestDestination::Document => "frame-src",
            RequestDestination::Script => "script-src-elem",
            RequestDestination::Fetch => "connect-src",
            RequestDestination::Style => "style-src-elem",
            RequestDestination::Image => "img-src",
            RequestDestination::Font => "font-src",
            RequestDestination::Video => "media-src",
        };
        if self.allows_url(directive, url, redirects) {
            Ok(())
        } else {
            Err(FetchError::new(
                FetchErrorKind::Network,
                format!("Content Security Policy blocked {directive} request"),
            ))
        }
    }
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
