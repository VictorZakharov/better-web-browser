//! Closed provenance and failure-policy checks for external WebGL conformance.
use super::*;

fn curated() -> Manifest {
    serde_json::from_str(include_str!("../../../tests/webgl/khronos-manifest.json")).unwrap()
}

#[test]
fn checked_in_webgl_gate_requires_native_capabilities_and_real_passes() {
    let manifest = curated();
    manifest.validate().unwrap();
    assert_eq!(manifest.policy.expectations, ExpectationsPolicy::Forbidden);
    assert!(manifest.policy.minimum_subtests >= 350);
    for test in &manifest.tests {
        assert_eq!(test.harness, HarnessKind::Khronos);
        assert_eq!(test.expected, ExpectedStatus::Pass);
        assert!(test.required_extension.is_some());
        assert!(test.reason.is_none());
    }
}

#[test]
fn webgl_provenance_rejects_nonofficial_repository_unpinned_revision_and_missing_license() {
    for repository in [
        "https://example.test/WebGL.git",
        "https://github.com/untrusted/WebGL.git",
    ] {
        let mut manifest = curated();
        manifest.upstream.repository = repository.into();
        assert!(manifest.validate().is_err());
    }
    for revision in ["main", "714857a", "", &"z".repeat(40)] {
        let mut manifest = curated();
        manifest.upstream.revision = revision.into();
        assert!(manifest.validate().is_err());
    }
    for license in ["", "unknown", "GPL-3.0"] {
        let mut manifest = curated();
        manifest.upstream.license = license.into();
        assert!(manifest.validate().is_err());
    }
}

#[test]
fn external_webgl_fixtures_cannot_cross_harness_or_expectation_boundaries() {
    let mut manifest = curated();
    manifest.tests[0].harness = HarnessKind::Testharness;
    assert!(manifest.validate().is_err());
    let mut manifest = curated();
    manifest.tests[0].expected = ExpectedStatus::Fail;
    manifest.tests[0].reason = Some("This must not silently weaken the curated gate".into());
    assert!(manifest.validate().is_err());
    let mut manifest = curated();
    manifest.support.push("sdk/tests/../../outside.js".into());
    assert!(manifest.validate().is_err());
}

#[test]
fn known_webgl_gaps_are_separate_explained_expectations_not_curated_passes() {
    let exploratory: Manifest = serde_json::from_str(include_str!(
        "../../../tests/webgl/exploratory-manifest.json"
    ))
    .unwrap();
    exploratory.validate().unwrap();
    assert_eq!(exploratory.policy.expectations, ExpectationsPolicy::Allowed);
    let gate = curated();
    for test in &exploratory.tests {
        assert_eq!(test.expected, ExpectedStatus::Fail);
        assert!(test.reason.as_ref().unwrap().len() > 32);
        assert!(!gate.tests.iter().any(|required| required.path == test.path));
    }
}
