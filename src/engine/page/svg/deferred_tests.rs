//! Hidden DOM remains authoritative; only its unnecessary raster work is deferred.
use super::*;
use crate::engine::dom::{self, ShadowRootMode};
use std::sync::Arc;

const URL: &str = "https://example.test/";

fn live(html: &str) -> Page {
    Page::from_dom(dom::parse(html), URL)
}

fn element(page: &Page, tag: &str) -> NodeRef {
    page.dom.elements_named(tag).next().unwrap()
}

fn refresh(page: &mut Page) {
    page.refresh_resources_for_viewport(800., 600.);
}

#[test]
fn live_dom_construction_defers_rasters_until_the_real_presentation_cascade() {
    let html = "<style>section{display:none}aside{color:blue}</style><section><svg width=10 height=10><rect width=10 height=10 fill=red /></svg></section><aside><svg width=10 height=10><rect width=10 height=10 fill=currentColor /></svg></aside>";
    let mut page = live(html);
    let svgs = page.dom.elements_named("svg").collect::<Vec<_>>();
    let hidden = inline_svg_key(&svgs[0]);
    let visible = inline_svg_key(&svgs[1]);
    assert!(page.inline_svg_versions.is_empty());
    assert!(!page.images.contains_key(&hidden) && !page.images.contains_key(&visible));
    refresh(&mut page);
    assert!(!page.images.contains_key(&hidden));
    assert!(!page.inline_svg_versions.contains_key(&svgs[0].id()));
    assert_eq!(&page.images[&visible].bgra[20..24], &[255, 0, 0, 255]);
    assert!(page.has_image_update(&visible));
    // Standalone full-page parse preserves the eager image metadata contract
    // used by immutable layout callers that do not run a live resource refresh.
    let standalone = Page::parse(html, URL);
    let svg = element(&standalone, "svg");
    assert!(standalone.images.contains_key(&inline_svg_key(&svg)));
}

#[test]
fn a_child_document_rasterizes_with_its_stylesheets_and_own_viewport() {
    use crate::engine::css::{StylesheetSource, media::MediaEnvironment};
    let dom =
        dom::parse("<svg width=10 height=10><rect width=10 height=10 fill=currentColor /></svg>");
    let svg = dom.elements_named("svg").next().unwrap();
    let key = inline_svg_key(&svg);
    let mut page = Page::from_frame_document(
        dom.document,
        URL,
        vec![StylesheetSource::injected(
            URL,
            "svg{color:blue}@media(min-width:500px){svg{display:none}}".into(),
        )],
        false,
        MediaEnvironment::new(600., 400., 1., false),
    );
    assert!(!page.images.contains_key(&key));
    page.refresh_resources_for_viewport(600., 400.);
    assert!(!page.images.contains_key(&key));
    page.refresh_resources_for_viewport(300., 400.);
    assert_eq!(&page.images[&key].bgra[20..24], &[255, 0, 0, 255]);
    assert!(page.has_image_update(&key));
}

#[test]
fn hidden_mutations_are_not_stamped_and_reveal_uses_current_dimensions_and_pixels() {
    let mut page = live(
        "<section><svg width=10 height=10><rect width=10 height=10 fill=red /></svg></section>",
    );
    refresh(&mut page);
    let svg = element(&page, "svg");
    let section = element(&page, "section");
    let rect = element(&page, "rect");
    let key = inline_svg_key(&svg);
    let old = page.images[&key].bgra.clone();
    let stamp = page.inline_svg_versions[&svg.id()];
    page.acknowledge_image_updates(std::slice::from_ref(&key));
    section.set_attr("style", "display:none");
    svg.set_attr("width", "20");
    rect.set_attr("width", "20");
    rect.set_attr("fill", "blue");
    refresh(&mut page);
    assert!(Arc::ptr_eq(&old, &page.images[&key].bgra));
    assert_eq!(page.inline_svg_versions[&svg.id()], stamp);
    assert!(!page.has_image_update(&key));
    section.set_attr("style", "display:block");
    refresh(&mut page);
    assert_eq!(
        (page.images[&key].width, page.images[&key].height),
        (20, 10)
    );
    assert_eq!(&page.images[&key].bgra[20..24], &[255, 0, 0, 255]);
    assert!(!Arc::ptr_eq(&old, &page.images[&key].bgra));
    assert!(page.has_image_update(&key));
}

#[test]
fn sibling_use_reads_hidden_definitions_without_rasterizing_the_definition_root() {
    let mut page = live(
        r##"<svg style='display:none' width=10 height=10><defs><rect id=shape width=10 height=10 fill=red /></defs></svg><svg width=10 height=10><use href='#shape' /></svg>"##,
    );
    refresh(&mut page);
    let svgs = page.dom.elements_named("svg").collect::<Vec<_>>();
    let hidden = inline_svg_key(&svgs[0]);
    let visible = inline_svg_key(&svgs[1]);
    assert!(!page.images.contains_key(&hidden));
    assert_eq!(&page.images[&visible].bgra[20..24], &[0, 0, 255, 255]);
    element(&page, "rect").set_attr("fill", "blue");
    refresh(&mut page);
    assert!(!page.images.contains_key(&hidden));
    assert_eq!(&page.images[&visible].bgra[20..24], &[255, 0, 0, 255]);
    assert!(page.has_image_update(&visible));
}

#[test]
fn inherited_color_and_new_stylesheets_are_resolved_before_a_hidden_reveal() {
    let mut page = live(
        "<section style='display:none;color:red'><svg width=10 height=10><rect width=10 height=10 fill=currentColor /></svg></section>",
    );
    refresh(&mut page);
    let svg = element(&page, "svg");
    let key = inline_svg_key(&svg);
    assert!(!page.images.contains_key(&key));
    page.add_stylesheet_from(
        "https://example.test/style.css",
        "section{color:blue!important}".into(),
    );
    element(&page, "section").set_attr("style", "display:block;color:red");
    refresh(&mut page);
    assert_eq!(&page.images[&key].bgra[20..24], &[255, 0, 0, 255]);
}

#[test]
fn visibility_and_opacity_are_not_display_none_shortcuts() {
    for style in ["opacity:0", "visibility:hidden"] {
        let mut page = live(&format!(
            "<section style='{style}'><svg width=10 height=10><rect style='visibility:visible' width=10 height=10 fill=red /></svg></section>"
        ));
        refresh(&mut page);
        let svg = element(&page, "svg");
        let key = inline_svg_key(&svg);
        assert!(page.images.contains_key(&key), "{style}");
        assert_eq!(&page.images[&key].bgra[20..24], &[0, 0, 255, 255]);
    }
}

#[test]
fn hidden_removal_retires_old_raster_and_publication_state() {
    let mut page = live(
        "<section><svg width=10 height=10><rect width=10 height=10 fill=red /></svg></section>",
    );
    refresh(&mut page);
    let svg = element(&page, "svg");
    let section = element(&page, "section");
    let key = inline_svg_key(&svg);
    section.set_attr("style", "display:none");
    assert!(Node::remove_child(&section, &svg));
    refresh(&mut page);
    assert!(!page.images.contains_key(&key));
    assert!(!page.inline_svg_versions.contains_key(&svg.id()));
    assert!(!page.has_image_update(&key));
    assert!(Node::append_child(&section, svg.clone()));
    refresh(&mut page);
    assert!(!page.images.contains_key(&key));
    section.set_attr("style", "display:block");
    refresh(&mut page);
    assert_eq!(&page.images[&key].bgra[20..24], &[0, 0, 255, 255]);
    assert!(page.has_image_update(&key));
}

#[test]
fn hidden_invalid_source_is_admitted_or_rejected_only_when_raster_work_is_needed() {
    let mut page = live(
        "<section style='display:none'><svg width=10 height=10><text>x</text></svg></section>",
    );
    let svg = element(&page, "svg");
    let key = inline_svg_key(&svg);
    Node::set_text_content(&element(&page, "text"), &"x".repeat(16_385));
    refresh(&mut page);
    assert!(!page.inline_svg_versions.contains_key(&svg.id()));
    assert!(
        !page
            .diagnostics
            .iter()
            .any(|row| row.starts_with("inline SVG "))
    );
    element(&page, "section").set_attr("style", "display:block");
    refresh(&mut page);
    assert!(!page.images.contains_key(&key));
    assert!(page.inline_svg_versions.contains_key(&svg.id()));
    let errors = page
        .diagnostics
        .iter()
        .filter(|row| row.starts_with("inline SVG "))
        .count();
    assert_eq!(errors, 1);
    refresh(&mut page);
    assert_eq!(
        page.diagnostics
            .iter()
            .filter(|row| row.starts_with("inline SVG "))
            .count(),
        errors
    );
}

#[test]
fn closed_shadow_svg_reveal_keeps_tree_local_reference_resolution() {
    let mut page = live("<x-host style='display:none'></x-host>");
    let host = element(&page, "x-host");
    let shadow = Node::attach_shadow(&host, ShadowRootMode::Closed, false, false, false).unwrap();
    Node::replace_inner_html(
        &shadow,
        r##"<svg style='display:none'><defs><rect id=shape width=10 height=10 fill=blue /></defs></svg><svg width=10 height=10><use href='#shape'/></svg>"##,
        true,
    );
    let svgs = Node::descendants(&shadow)
        .filter(|node| node.tag_name() == Some("svg"))
        .collect::<Vec<_>>();
    let key = inline_svg_key(&svgs[1]);
    refresh(&mut page);
    assert!(!page.images.contains_key(&key));
    host.set_attr("style", "display:block");
    refresh(&mut page);
    assert!(!page.images.contains_key(&inline_svg_key(&svgs[0])));
    assert_eq!(&page.images[&key].bgra[20..24], &[255, 0, 0, 255]);
}

#[test]
fn fullscreen_svg_descendants_are_not_deferred_by_a_hidden_assigned_slot() {
    let mut page = live(
        "<x-host><main slot=content><svg width=10 height=10><rect width=10 height=10 fill=red /></svg></main></x-host>",
    );
    let host = element(&page, "x-host");
    let target = element(&page, "main");
    let svg = element(&page, "svg");
    let key = inline_svg_key(&svg);
    let shadow = Node::attach_shadow(&host, ShadowRootMode::Open, false, false, false).unwrap();
    Node::replace_inner_html(
        &shadow,
        "<slot name=content style='display:none'></slot>",
        true,
    );
    refresh(&mut page);
    assert!(!page.images.contains_key(&key));
    target.set_fullscreen(true);
    refresh(&mut page);
    assert_eq!(&page.images[&key].bgra[20..24], &[0, 0, 255, 255]);
    let old = page.images[&key].bgra.clone();
    target.set_fullscreen(false);
    element(&page, "rect").set_attr("fill", "blue");
    refresh(&mut page);
    assert!(Arc::ptr_eq(&old, &page.images[&key].bgra));
    target.set_fullscreen(true);
    refresh(&mut page);
    assert_eq!(&page.images[&key].bgra[20..24], &[255, 0, 0, 255]);
}
