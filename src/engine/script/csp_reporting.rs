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
    if external {
        // External Fetch/CSP admission has its own reporting path. It must not
        // be conflated with an inline-script violation or leak a blocked URL.
        return;
    }
    let (violations, node_id, document_url) = {
        let state = host.borrow();
        let violations = state
            .policy
            .inline_script_violations(script.node.attr("nonce").as_deref())
            .into_iter()
            .map(|violation| {
                (
                    violation.original_policy.to_owned(),
                    violation.report_sample,
                )
            })
            .collect::<Vec<_>>();
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
        let sample = if report_sample {
            script.code.chars().take(40).collect::<String>()
        } else {
            String::new()
        };
        let init = serde_json::json!({
            "documentURI": document_url,
            "blockedURI": "inline",
            "effectiveDirective": "script-src-elem",
            "violatedDirective": "script-src-elem",
            "originalPolicy": original_policy,
            "sourceFile": document_url,
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
