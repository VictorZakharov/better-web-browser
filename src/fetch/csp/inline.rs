//! CSP3 inline script checks.
//! https://www.w3.org/TR/CSP3/#match-element-to-source-list

use super::{PolicyContainer, has_keyword, nonce_matches, sources};
#[cfg(test)]
mod tests;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InlineScriptViolation<'a> {
    pub original_policy: &'a str,
    pub report_sample: bool,
}

#[derive(Clone, Copy)]
enum InlineType {
    ScriptElement { parser_inserted: bool },
    ScriptAttribute,
}

impl InlineType {
    fn directive(self) -> &'static str {
        match self {
            Self::ScriptElement { .. } => "script-src-elem",
            Self::ScriptAttribute => "script-src-attr",
        }
    }

    fn is_element(self) -> bool {
        !matches!(self, Self::ScriptAttribute)
    }
}

impl PolicyContainer {
    /// Source-less legacy checks can only match a nonce or `unsafe-inline`.
    pub fn allows_inline(&self, attribute: bool) -> bool {
        self.allows_inline_with_nonce(attribute, None)
    }

    pub fn allows_inline_with_nonce(&self, attribute: bool, nonce: Option<&str>) -> bool {
        let kind = if attribute {
            InlineType::ScriptAttribute
        } else {
            InlineType::ScriptElement {
                parser_inserted: true,
            }
        };
        self.allows_inline_source(kind, nonce, None)
    }

    pub fn allows_inline_script(
        &self,
        nonce: Option<&str>,
        source: &str,
        parser_inserted: bool,
    ) -> bool {
        self.allows_inline_source(
            InlineType::ScriptElement { parser_inserted },
            nonce,
            Some(source),
        )
    }

    fn allows_inline_source(
        &self,
        kind: InlineType,
        nonce: Option<&str>,
        source: Option<&str>,
    ) -> bool {
        self.policies.iter().all(|policy| {
            policy
                .list(kind.directive())
                .is_none_or(|list| matches_inline(list, kind, nonce, source))
        })
    }

    /// Each enforcing policy reports separately, preserving its original serialization.
    pub fn inline_script_violations(&self, nonce: Option<&str>) -> Vec<InlineScriptViolation<'_>> {
        self.inline_script_violations_for_source(nonce, None, true)
    }

    pub fn inline_script_violations_for_source(
        &self,
        nonce: Option<&str>,
        source: Option<&str>,
        parser_inserted: bool,
    ) -> Vec<InlineScriptViolation<'_>> {
        self.policies
            .iter()
            .filter_map(|policy| {
                let list = policy.list("script-src-elem")?;
                if matches_inline(
                    list,
                    InlineType::ScriptElement { parser_inserted },
                    nonce,
                    source,
                ) {
                    return None;
                }
                Some(InlineScriptViolation {
                    original_policy: &policy.serialized,
                    report_sample: has_keyword(list, "'report-sample'"),
                })
            })
            .collect()
    }
}

fn matches_inline(
    list: &[String],
    kind: InlineType,
    nonce: Option<&str>,
    source: Option<&str>,
) -> bool {
    let strict_dynamic = has_keyword(list, "'strict-dynamic'");
    let has_nonce_or_hash = list
        .iter()
        .any(|source| sources::nonce_value(source).is_some() || sources::hash_source(source));
    if !strict_dynamic && !has_nonce_or_hash && has_keyword(list, "'unsafe-inline'") {
        return true;
    }
    // CSP3 §6.7.3.1 also rejects a nonceable element when any raw attribute
    // contains a dangling <script token. The current DOM has already
    // discarded duplicate raw attributes, so that nonceability guard is not
    // implemented here. This must be closed at tokenization before full CSP3.
    if kind.is_element() && nonce_matches(list, nonce) {
        return true;
    }
    // CSP3 §6.7.3.3 allows a non-parser-inserted inline script under strict-dynamic.
    if matches!(
        kind,
        InlineType::ScriptElement {
            parser_inserted: false
        }
    ) && strict_dynamic
    {
        return true;
    }
    kind.is_element()
        && source.is_some_and(|source| {
            list.iter()
                .any(|expression| sources::hash_matches(expression, source))
        })
}
