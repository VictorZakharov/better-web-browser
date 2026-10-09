use super::*;
use crate::engine::invalidation::MutationKind;

fn page(extra: &str) -> (Page, NodeRef) {
    let mut page = Page::parse_scripted(
        &format!(
            "<style>{extra}</style><section><div id=target style='color:red;padding:0'>text</div></section>"
        ),
        "https://example.test/",
    );
    page.refresh_resources_for_viewport(800., 600.);
    let target = page.dom.elements_named("div").next().unwrap();
    (page, target)
}
fn dirty(target: &NodeRef) -> RenderInvalidation {
    RenderInvalidation {
        roots: vec![target.parent().unwrap().id()],
        impact: MutationKind::Attribute("style").impact(),
        mutation_count: 1,
        ..Default::default()
    }
}

#[test]
fn same_cascade_output_retains_resources_but_does_run_the_cascade() {
    let (mut page, target) = page("");
    let resources = page.resources.clone();
    target.set_attr("style", "padding:0;color:red");
    let (stats, unchanged) = page.refresh_presentation_styles(800., 600., &dirty(&target));
    assert!(unchanged);
    assert!(stats.recomputed_styles > 0);
    assert_eq!(stats.changed_styles, 0);
    assert_eq!(page.resources, resources);
    assert!(!stats.full_rebuild);
}

#[test]
fn native_animation_overlay_equality_is_proved_against_the_real_cascade() {
    let (mut page, target) = page("");
    target.set_animation_style("color:red;padding:0");
    let mut invalidation = dirty(&target);
    invalidation.impact = MutationKind::StyleOverlay.impact();
    let (stats, unchanged) = page.refresh_presentation_styles(800., 600., &invalidation);
    assert!(unchanged && stats.recomputed_styles > 0);
    assert!(
        target
            .attr("style")
            .is_some_and(|value| value == "color:red;padding:0")
    );
    target.set_animation_style("color:blue;padding:1px");
    let (stats, unchanged) = page.refresh_presentation_styles(800., 600., &invalidation);
    assert!(!unchanged && stats.changed_styles > 0 && stats.layout_changed);
}

#[test]
fn any_paint_geometry_visibility_or_image_difference_uses_full_resource_path() {
    for declarations in [
        "color:blue",
        "padding:1px",
        "display:none",
        "visibility:hidden",
        "opacity:.5",
        "transform:translateX(1px)",
        "font-size:32px",
        "background-image:url(icon.png)",
        "--ink:blue;color:var(--ink)",
    ] {
        let (mut page, target) = page("");
        target.set_attr("style", declarations);
        let (stats, unchanged) = page.refresh_presentation_styles(800., 600., &dirty(&target));
        assert!(!unchanged, "{declarations}");
        assert!(
            stats.changed_styles > 0 || stats.layout_changed || stats.non_deferable_paint_changes,
            "lost first cascade evidence: {declarations}"
        );
        if declarations.contains("icon.png") {
            assert!(page.resources.iter().any(
                |r| matches!(r,PageResource::Image{url}if url=="https://example.test/icon.png")
            ));
        }
    }
}

#[test]
fn attribute_backed_pseudo_content_invalidates_even_when_element_style_is_equal() {
    let (mut page, target) = page("#target::before{content:attr(style)}");
    target.set_attr("style", "padding:0;color:red");
    let (stats, unchanged) = page.refresh_presentation_styles(800., 600., &dirty(&target));
    assert!(!unchanged);
    assert!(stats.layout_changed || stats.non_deferable_paint_changes);
}

#[test]
fn non_css_mutations_viewport_rules_and_unknown_roots_never_qualify() {
    for kind in [
        MutationKind::Attribute("class"),
        MutationKind::ChildList,
        MutationKind::CharacterData,
        MutationKind::State,
    ] {
        let (mut page, target) = page("");
        let mut invalidation = dirty(&target);
        invalidation.impact = kind.impact();
        let (_, unchanged) = page.refresh_presentation_styles(800., 600., &invalidation);
        assert!(!unchanged, "{kind:?}");
    }
    let (mut page, target) = page("");
    assert!(
        !page
            .refresh_presentation_styles(801., 600., &dirty(&target))
            .1
    );
    let mut invalidation = dirty(&target);
    invalidation.rebuild_style_rules = true;
    assert!(
        !page
            .refresh_presentation_styles(801., 600., &invalidation)
            .1
    );
    invalidation.roots.clear();
    invalidation.rebuild_style_rules = false;
    assert!(
        !page
            .refresh_presentation_styles(801., 600., &invalidation)
            .1
    );
}

#[test]
fn scoped_rule_sheets_keep_the_general_path_even_for_identical_styles() {
    let (mut page, target) = page("@scope (section){#target{color:red}}");
    target.set_attr("style", "padding:0;color:red");
    assert!(
        !page
            .refresh_presentation_styles(800., 600., &dirty(&target))
            .1
    );
}

#[test]
fn svg_definitions_and_slot_roots_keep_the_general_path() {
    for content in [
        "<svg width=1 height=1><rect width=1 height=1 fill=currentColor /></svg>",
        "<slot></slot>",
        "<base href='/other/'>",
    ] {
        let mut page = Page::parse_scripted(
            &format!(
                "<section><div id=target style='color:red;padding:0'>{content}</div></section>"
            ),
            "https://example.test/",
        );
        let target = page.dom.elements_named("div").next().unwrap();
        page.refresh_resources_for_viewport(800., 600.);
        target.set_attr("style", "padding:0;color:red");
        assert!(
            !page
                .refresh_presentation_styles(800., 600., &dirty(&target))
                .1,
            "{content}"
        );
    }
}

#[test]
fn relational_rules_refresh_ancestors_and_nested_selector_dependencies() {
    for selector in [
        "html:has([style^='padding']) body",
        ":is(html:has([style^='padding'])) body",
        "html:not(:not(:has([style^='padding']))) body",
        "body:nth-child(1 of :has([style^='padding']))",
    ] {
        let (mut page, target) = page(&format!("{selector}{{padding:17px}}"));
        let body = page.dom.elements_named("body").next().unwrap();
        let before = page
            .cached_style_for_viewport(800., 600.)
            .unwrap()
            .get(&body)
            .clone();
        target.set_attr("style", "padding:0;color:red");
        let (stats, unchanged) = page.refresh_presentation_styles(800., 600., &dirty(&target));
        assert!(!unchanged, "{selector}");
        assert!(
            stats.changed_styles > 0 && stats.layout_changed,
            "{selector}"
        );
        let cached = page.cached_style_for_viewport(800., 600.).unwrap();
        assert_ne!(cached.get(&body), &before, "{selector}");
        assert_eq!(
            cached.styles,
            page.style_for_viewport(800., 600.).styles,
            "{selector}"
        );
    }
}

#[test]
fn unrelated_relational_selectors_keep_style_and_overlay_refreshes_local() {
    for overlay in [false, true] {
        let mut page = Page::parse_scripted(
            &format!(
                "<style>html:has(.active) body{{color:red}}</style>\
                 <section><div class=active style='padding:0;color:red'>text</div></section>\
                 <aside>{}</aside>",
                "<p>unaffected</p>".repeat(100)
            ),
            "https://example.test/",
        );
        page.refresh_resources_for_viewport(800., 600.);
        let target = page.dom.elements_named("div").next().unwrap();
        let mut invalidation = dirty(&target);
        if overlay {
            target.set_animation_style("padding:0;color:red");
            invalidation.impact = MutationKind::StyleOverlay.impact();
        } else {
            target.set_attr("style", "color:red;padding:0");
        }
        let (stats, unchanged) = page.refresh_presentation_styles(800., 600., &invalidation);
        assert!(unchanged && stats.recomputed_styles > 0);
        assert!(
            stats.recomputed_styles < 10,
            "unrelated :has() widened: {stats:?}"
        );
        assert_eq!(
            page.cached_style_for_viewport(800., 600.).unwrap().styles,
            page.style_for_viewport(800., 600.).styles
        );

        // A later class change has no declaration-only proof. It must invalidate
        // the ancestor match and styles elsewhere in the document.
        target.set_attr("class", "inactive");
        let mut invalidation = dirty(&target);
        invalidation.impact = MutationKind::Attribute("class").impact();
        let stats =
            page.refresh_resources_after_invalidation_for_viewport(800., 600., &invalidation);
        assert!(stats.recomputed_styles > 100);
        assert_eq!(
            page.cached_style_for_viewport(800., 600.).unwrap().styles,
            page.style_for_viewport(800., 600.).styles
        );
    }
}

#[test]
fn svg_and_fullscreen_ancestors_do_not_qualify_even_with_an_html_dirty_root() {
    let mut svg_page = Page::parse_scripted(
        "<svg><foreignObject><section><div style='color:red;padding:0'>text</div></section></foreignObject></svg>",
        "https://example.test/",
    );
    let target = svg_page.dom.elements_named("div").next().unwrap();
    svg_page.refresh_resources_for_viewport(800., 600.);
    target.set_attr("style", "padding:0;color:red");
    assert!(
        !svg_page
            .refresh_presentation_styles(800., 600., &dirty(&target))
            .1
    );

    let (mut page, target) = page("");
    let body = page.dom.elements_named("body").next().unwrap();
    body.set_fullscreen(true);
    page.refresh_resources_for_viewport(800., 600.);
    target.set_attr("style", "padding:0;color:red");
    assert!(
        !page
            .refresh_presentation_styles(800., 600., &dirty(&target))
            .1
    );
}
