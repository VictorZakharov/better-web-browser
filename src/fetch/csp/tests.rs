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
fn unimplemented_features_refuse_policy_instead_of_bypassing_it() {
    for value in [
        "script-src 'nonce-secret'",
        "script-src 'sha256-hash'",
        "script-src 'strict-dynamic'",
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
