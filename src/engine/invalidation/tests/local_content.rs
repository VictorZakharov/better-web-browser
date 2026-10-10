use super::*;

#[test]
fn local_content_proof_survives_only_known_coalesced_categories() {
    let text = MutationKind::TextChildren.impact();
    let graphics = MutationKind::Attribute("d").impact();
    let style = MutationKind::StyleOverlay.impact();
    assert!(text.is_text_only() && text.affects_style() && text.affects_intrinsic_size());
    assert!(graphics.is_local_content_only());
    let combined = text.union(graphics).union(style);
    assert!(combined.is_local_content_only());
    assert!(!combined.is_style_only() && !combined.is_text_only());
    assert_eq!(combined.union(InvalidationImpact::default()), combined);
    for unknown in [
        MutationKind::Attribute("class"),
        MutationKind::Attribute("data-active"),
        MutationKind::ChildList,
        MutationKind::State,
        MutationKind::CharacterData,
    ] {
        assert!(
            !combined.union(unknown.impact()).is_local_content_only(),
            "{unknown:?}"
        );
    }
}
