//! Native CSP admission reports violations to the document realm after the
//! responsible script task; sandbox denials are not CSP violations.
use super::super::*;

pub(super) fn queue_script_violation(
    context: &mut Context,
    host: &Rc<RefCell<HostState>>,
    outcome: &mut ScriptOutcome,
    script: &ScriptInput,
    external: bool,
) {
    let (violations, node_id, document_url) = {
        let state = host.borrow();
        let violations = if external {
            let source = crate::fetch::csp::ScriptSource {
                nonce: script.node.attr("nonce"),
                parser_inserted: script
                    .node
                    .element()
                    .is_some_and(|element| element.script_parser_inserted.get()),
            };
            state
                .policy
                .script_url_violations("script-src-elem", &script.source_url, 0, &source)
                .into_iter()
                .map(|violation| (violation.original_policy.to_owned(), false))
                .collect::<Vec<_>>()
        } else {
            state
                .policy
                .inline_script_violations(script.node.attr("nonce").as_deref())
                .into_iter()
                .map(|violation| {
                    (
                        violation.original_policy.to_owned(),
                        violation.report_sample,
                    )
                })
                .collect::<Vec<_>>()
        };
        let node_id = state
            .node_ids
            .get(&script.node.id())
            .copied()
            .unwrap_or_default();
        (violations, node_id, strip_report_url(&state.document_url))
    };
    if violations.is_empty() {
        return;
    }
    for (original_policy, report_sample) in violations {
        let sample = if report_sample && !external {
            script.code.chars().take(40).collect::<String>()
        } else {
            String::new()
        };
        let blocked_uri = if external {
            strip_blocked_url(&script.source_url, &document_url)
        } else {
            "inline".to_owned()
        };
        let init = serde_json::json!({
            "documentURI": document_url,
            "blockedURI": blocked_uri,
            "effectiveDirective": "script-src-elem",
            "violatedDirective": "script-src-elem",
            "originalPolicy": original_policy,
            "sourceFile": if external { "" } else { &document_url },
            "sample": sample,
            "disposition": "enforce",
        });
        let dispatch = format!("document.__queuePolicyViolation({node_id}, {init});");
        if let Err(error) = context.eval(Source::from_bytes(&dispatch)) {
            outcome.errors.push(format!(
                "{}: queue CSP violation event: {error}",
                script.source_url
            ));
        }
    }
}

fn strip_blocked_url(source: &str, document_url: &str) -> String {
    let Ok(mut url) = url::Url::parse(source) else {
        return String::new();
    };
    if !matches!(url.scheme(), "http" | "https") {
        return format!("{}:", url.scheme());
    }
    let _ = url.set_username("");
    let _ = url.set_password(None);
    url.set_fragment(None);
    let same_origin =
        url::Url::parse(document_url).is_ok_and(|document| document.origin() == url.origin());
    if same_origin {
        url.to_string()
    } else {
        url.origin().ascii_serialization()
    }
}

fn strip_report_url(source: &str) -> String {
    let Ok(mut url) = url::Url::parse(source) else {
        return source.to_owned();
    };
    if !matches!(url.scheme(), "http" | "https") {
        return format!("{}:", url.scheme());
    }
    let _ = url.set_username("");
    let _ = url.set_password(None);
    url.set_fragment(None);
    url.to_string()
}
