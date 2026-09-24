use super::*;
fn policy(values: &[&str]) -> PolicyContainer {
    let mut headers = HeaderList::new();
    for value in values {
        headers.append("content-security-policy", value).unwrap();
    }
    PolicyContainer::from_headers("https://example.test/doc", &headers).unwrap()
}
#[test]
fn directives_use_correct_fallback_and_intersect_multiple_policies() {
    let policies = policy(&[
        "default-src 'none'; script-src 'self' 'unsafe-inline'; child-src https://child.test; form-action https://submit.test",
        "script-src https://example.test/a/",
    ]);
    assert!(policies.allows_url("script-src-elem", "https://example.test/a/main.js", 0));
    assert!(!policies.allows_url("script-src-elem", "https://example.test/b/main.js", 0));
    assert!(!policies.allows_inline(false));
    assert!(!policies.allows_eval());
    assert!(policies.allows_url("frame-src", "https://child.test/frame", 0));
    assert!(!policies.allows_url("connect-src", "https://example.test/", 0));
    assert!(policies.allows_url("base-uri", "https://other.test/", 0));
    assert!(!policies.allows_url("form-action", "https://other.test/", 0));
}

#[test]
fn worker_src_uses_its_own_directive_then_child_script_and_default_fallbacks() {
    let destination = RequestDestination::Worker;
    let url = "https://workers.example.test/entry.js";
    for directive in ["worker-src", "child-src", "script-src", "default-src"] {
        let allowed = policy(&[&format!("{directive} https://workers.example.test")]);
        assert!(
            allowed.check_request(destination, url, 0).is_ok(),
            "{directive}"
        );
        assert!(
            allowed
                .check_request(destination, "https://other.example.test/entry.js", 0)
                .is_err(),
            "{directive}"
        );
    }
    let override_policy = policy(&[
        "worker-src 'none'; child-src https://workers.example.test; script-src https://workers.example.test",
    ]);
    assert!(override_policy.check_request(destination, url, 0).is_err());
    let no_worker_restriction = policy(&["connect-src 'none'"]);
    assert!(
        no_worker_restriction
            .check_request(destination, url, 0)
            .is_ok()
    );
}
#[test]
fn duplicate_directives_keep_first_and_comma_policies_intersect() {
    let policies =
        policy(&["script-src 'none'; script-src 'unsafe-inline', connect-src https://api.test"]);
    assert!(!policies.allows_inline(false));
    assert!(policies.allows_url("connect-src", "https://api.test/x", 0));
    assert!(!policies.allows_url("connect-src", "https://other.test/", 0));
}
#[test]
fn source_matching_preserves_scheme_host_port_path_and_redirect_rules() {
    let policies = policy(&[
        "connect-src https://*.example.test:8443/a/%62/ http://upgrade.test https://exact.test/file",
    ]);
    for allowed in [
        "https://sub.example.test:8443/a/b/c",
        "https://upgrade.test/",
        "https://exact.test/file",
    ] {
        assert!(policies.allows_url("connect-src", allowed, 0), "{allowed}");
    }
    for denied in [
        "http://sub.example.test:8443/a/b/c",
        "https://example.test:8443/a/b/c",
        "https://sub.example.test/a/b/c",
        "https://sub.example.test:8443/a%2fb/c",
        "https://exact.test/file2",
        "https://exact.test/file/more",
    ] {
        assert!(!policies.allows_url("connect-src", denied, 0), "{denied}");
    }
    assert!(policies.allows_url("connect-src", "https://exact.test/redirect", 1));
    assert!(!policies.allows_url("connect-src", "https://other.test/redirect", 1));
}
#[test]
fn connect_src_self_matches_secure_websocket_but_not_foreign_or_downgraded_hosts() {
    let secure = policy(&["connect-src 'self'"]);
    assert!(secure.allows_url("connect-src", "wss://example.test/socket", 0));
    assert!(!secure.allows_url("connect-src", "ws://example.test/socket", 0));
    assert!(!secure.allows_url("connect-src", "wss://other.test/socket", 0));
    assert!(!secure.allows_url("connect-src", "wss://example.test:8443/socket", 0));

    let mut headers = HeaderList::new();
    headers
        .append("content-security-policy", "connect-src 'self'")
        .unwrap();
    let insecure = PolicyContainer::from_headers("http://example.test/", &headers).unwrap();
    assert!(insecure.allows_url("connect-src", "ws://example.test/socket", 0));
    assert!(insecure.allows_url("connect-src", "wss://example.test/socket", 0));
}
#[test]
fn unimplemented_features_refuse_policy_instead_of_bypassing_it() {
    for value in [
        "script-src 'sha256-hash'",
        "script-src 'trusted-types-eval'",
        "require-trusted-types-for 'script'",
        "sandbox allow-scripts",
    ] {
        let mut headers = HeaderList::new();
        headers.append("content-security-policy", value).unwrap();
        assert!(
            PolicyContainer::from_headers("https://example.test/", &headers).is_err(),
            "{value}"
        );
    }
}

#[test]
fn parses_nonce_strict_dynamic_and_reporting_without_relaxing_enforcement() {
    let policy = policy(&[
        "script-src 'report-sample' 'nonce-AbC123=' 'unsafe-inline' 'strict-dynamic' https: 'unsafe-eval'; object-src 'none'; base-uri 'self'; report-uri https://reports.example.test/csp",
    ]);
    assert!(!policy.allows_inline(false));
    assert!(policy.allows_inline_with_nonce(false, Some("AbC123=")));
    assert!(!policy.allows_url("script-src-elem", "https://cdn.example.test/app.js", 0));
    assert!(policy.allows_script_url(
        "script-src-elem",
        "https://cdn.example.test/app.js",
        0,
        &ScriptSource {
            nonce: Some("AbC123=".into()),
            parser_inserted: true,
        },
    ));
    assert!(policy.allows_eval());
    assert!(!policy.allows_url("object-src", "https://example.test/plugin", 0));
    assert!(policy.allows_url("base-uri", "https://example.test/path", 0));
}

fn strict_policy() -> PolicyContainer {
    PolicyContainer {
        policies: vec![Policy {
            origin: url::Url::parse("https://example.test/").unwrap(),
            directives: HashMap::from([(
                "script-src".into(),
                vec![
                    "'report-sample'".into(),
                    "'nonce-AbC123='".into(),
                    "'unsafe-inline'".into(),
                    "'strict-dynamic'".into(),
                    "https:".into(),
                ],
            )]),
            mixed_content: false,
        }],
    }
}

#[test]
fn strict_dynamic_requires_a_nonce_for_parser_scripts_and_ignores_hosts() {
    let policy = strict_policy();
    let url = "https://cdn.example.test/app.js";
    assert!(!policy.allows_script_url("script-src-elem", url, 0, &ScriptSource::default()));
    assert!(!policy.allows_url("script-src-elem", url, 0));
    assert!(policy.allows_script_url(
        "script-src-elem",
        url,
        0,
        &ScriptSource {
            nonce: Some("AbC123=".into()),
            parser_inserted: true,
        },
    ));
    assert!(!policy.allows_script_url(
        "script-src-elem",
        url,
        0,
        &ScriptSource {
            nonce: Some("abc123=".into()),
            parser_inserted: true,
        },
    ));
    assert!(policy.allows_script_url(
        "script-src-elem",
        url,
        0,
        &ScriptSource {
            nonce: None,
            parser_inserted: false,
        },
    ));
    assert!(!policy.allows_script_url(
        "script-src-elem",
        "http://cdn.example.test/app.js",
        0,
        &ScriptSource {
            nonce: None,
            parser_inserted: true,
        },
    ));
}

#[test]
fn nonce_disables_unsafe_inline_but_authorizes_matching_script_element() {
    let policy = strict_policy();
    assert!(!policy.allows_inline(false));
    assert!(!policy.allows_inline(true));
    assert!(policy.allows_inline_with_nonce(false, Some("AbC123=")));
    assert!(!policy.allows_inline_with_nonce(false, Some("abc123=")));
    assert!(!policy.allows_inline_with_nonce(true, Some("AbC123=")));
}
