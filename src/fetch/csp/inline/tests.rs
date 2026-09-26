use super::*;
use crate::fetch::HeaderList;
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use sha2::{Digest, Sha256, Sha384, Sha512};

fn policy(values: &[&str]) -> PolicyContainer {
    let mut headers = HeaderList::new();
    for value in values {
        headers.append("content-security-policy", value).unwrap();
    }
    PolicyContainer::from_headers("https://example.test/", &headers).unwrap()
}

#[test]
fn script_hashes_match_exact_utf8_text_with_all_three_algorithms() {
    let source = "π = 'hello';\n";
    let sha256 = STANDARD.encode(Sha256::digest(source.as_bytes()));
    let sha384 = STANDARD.encode(Sha384::digest(source.as_bytes()));
    let sha512 = STANDARD.encode(Sha512::digest(source.as_bytes()));
    for expression in [
        format!("'sha256-{sha256}'"),
        format!("'SHA384-{sha384}'"),
        format!("'sha512-{sha512}'"),
    ] {
        let policy = policy(&[&format!("script-src {expression}")]);
        assert!(
            policy.allows_inline_script(None, source, true),
            "{expression}"
        );
        assert!(!policy.allows_inline_script(None, "π = 'hello';", true));
        assert!(!policy.allows_inline_script(None, "π = 'hello';\r\n", true));
    }
}

#[test]
fn base64url_hash_is_equivalent_but_invalid_padding_cannot_match() {
    let source = "hash source 2";
    let standard = STANDARD.encode(Sha256::digest(source.as_bytes()));
    assert!(standard.contains('+') && standard.contains('/'));
    let url_safe = standard.replace('+', "-").replace('/', "_");
    assert!(
        policy(&[&format!("script-src 'sha256-{url_safe}'")])
            .allows_inline_script(None, source, true)
    );
    assert!(
        !policy(&[&format!("script-src 'sha256-{standard}='")])
            .allows_inline_script(None, source, true)
    );
}

#[test]
fn element_directives_override_fallbacks_and_policies_intersect() {
    let source = "allowed()";
    let hash = STANDARD.encode(Sha256::digest(source.as_bytes()));
    let policies = policy(&[
        &format!("default-src 'none'; script-src 'unsafe-inline'; script-src-elem 'sha256-{hash}'"),
        "script-src-elem 'nonce-ok'",
    ]);
    assert!(!policies.allows_inline_script(None, source, true));
    assert!(policies.allows_inline_script(Some("ok"), source, true));
    assert!(!policies.allows_inline_script(Some("ok"), "different()", true));
    let blocked = policies.inline_script_violations_for_source(None, Some(source), true);
    assert_eq!(blocked.len(), 1);
    assert_eq!(blocked[0].original_policy, "script-src-elem 'nonce-ok'");
}

#[test]
fn hash_and_nonce_disable_unsafe_inline_but_strict_dynamic_allows_dynamic_script() {
    let hash = STANDARD.encode(Sha256::digest(b"approved()"));
    let hashed = policy(&[&format!("script-src 'unsafe-inline' 'sha256-{hash}'")]);
    assert!(hashed.allows_inline_script(None, "approved()", true));
    assert!(!hashed.allows_inline_script(None, "other()", true));
    let nonced = policy(&["script-src 'unsafe-inline' 'nonce-ok'"]);
    assert!(nonced.allows_inline_script(Some("ok"), "other()", true));
    assert!(!nonced.allows_inline_script(None, "other()", true));
    let dynamic = policy(&["script-src 'strict-dynamic' 'nonce-ok'"]);
    assert!(!dynamic.allows_inline_script(None, "other()", true));
    assert!(dynamic.allows_inline_script(None, "other()", false));
}
