use super::*;

#[test]
fn inline_style_evidence_is_identity_safe_commutative_and_mixed_fail_closed() {
    let style = MutationKind::Attribute("style").impact();
    assert!(style.is_style_only());
    assert!(MutationKind::Attribute("STYLE").impact().is_style_only());
    let overlay = MutationKind::StyleOverlay.impact();
    assert_eq!(style, overlay);
    assert!(style.union(overlay).is_style_only());
    assert!(style.affects_style() && style.affects_layout() && style.affects_paint());
    assert!(!style.affects_intrinsic_size());
    assert_eq!(style.union(InvalidationImpact::default()), style);
    assert_eq!(InvalidationImpact::default().union(style), style);
    assert_eq!(
        InvalidationImpact::default().union(InvalidationImpact::default()),
        InvalidationImpact::default()
    );
    assert_eq!(style.union(style), style);
    for other in [
        MutationKind::Attribute("class"),
        MutationKind::Attribute("width"),
        MutationKind::CharacterData,
        MutationKind::ChildList,
        MutationKind::Stylesheet,
        MutationKind::Viewport,
        MutationKind::State,
        MutationKind::PointerDesignation,
    ] {
        let mixed = style.union(other.impact());
        assert_eq!(mixed, other.impact().union(style));
        assert!(!mixed.is_style_only(), "{other:?}");
        assert_eq!(mixed.union(style), style.union(mixed));
        assert!(
            !mixed.union(style).is_style_only(),
            "later CSS must not restore proof"
        );
    }
}

#[test]
fn render_invalidation_merges_keep_style_only_only_for_complete_css_batches() {
    let document = crate::engine::dom::parse("<div id=a></div><div id=b></div>");
    let ids: Vec<_> = document
        .elements_named("div")
        .map(|node| node.id())
        .collect();
    let css = |index: usize| RenderInvalidation {
        roots: vec![ids[index]],
        impact: MutationKind::Attribute("style").impact(),
        ..Default::default()
    };
    let mut both = css(0);
    both.merge_conservatively(css(1), document.document.id());
    assert!(both.impact.is_style_only());
    for style_first in [true, false] {
        let content = RenderInvalidation {
            roots: vec![ids[1]],
            impact: MutationKind::CharacterData.impact(),
            ..Default::default()
        };
        let (mut first, second) = if style_first {
            (css(0), content)
        } else {
            (content, css(0))
        };
        first.merge_conservatively(second, document.document.id());
        assert!(!first.impact.is_style_only());
        first.merge_conservatively(css(1), document.document.id());
        assert!(!first.impact.is_style_only());
    }
}
