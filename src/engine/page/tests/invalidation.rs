use super::super::*;
use crate::engine::css::Color;
use crate::engine::dom::Node;
use crate::engine::invalidation::{MutationKind, RenderInvalidation};
use crate::engine::layout::{DisplayItem, FontSpec, TextMeasurer, layout_page};

struct FixedMeasurer;

impl TextMeasurer for FixedMeasurer {
    fn measure(&mut self, text: &str, font: &FontSpec) -> (f32, f32) {
        (text.chars().count() as f32 * font.size * 0.5, font.size)
    }
}

fn element_with_id(page: &Page, tag: &str, id: &str) -> NodeRef {
    page.dom
        .elements_named(tag)
        .find(|node| node.attr("id").as_deref() == Some(id))
        .unwrap()
}

#[test]
fn refreshes_only_the_invalidated_style_subtree_without_stale_values() {
    let mut page = Page::parse(
        r#"<style>.hot { color: #123456 } .branch { font-size: 24px }</style>
            <main><section><div id="target"><span>child</span></div></section>
            <aside><p>unrelated</p><p>unrelated</p><p>unrelated</p></aside></main>"#,
        "https://example.com/",
    );
    let full = page.refresh_resources(800.0);
    let target = element_with_id(&page, "div", "target");
    let section = target.parent().unwrap();
    target.set_attr("class", "hot branch");

    let stats = page.refresh_resources_after_invalidation(
        800.0,
        &RenderInvalidation {
            roots: vec![section.id()],
            impact: MutationKind::Attribute("class").impact(),
            mutation_count: 1,
            rebuild_style_rules: false,
            removed_nodes: Vec::new(),
        },
    );

    assert!(!stats.full_rebuild);
    assert!(stats.recomputed_styles < full.total_styles);
    assert_eq!(stats.recomputed_styles, stats.invalidated_nodes);
    let style = page.cached_style(800.0).unwrap().get(&target);
    assert_eq!(style.color, Color::rgb(0x12, 0x34, 0x56));
    assert_eq!(style.font_size, 24.0);
}

#[test]
fn refreshes_disjoint_component_subtrees_without_widening_to_the_document() {
    let mut page = Page::parse(
        r#"<style>.hot { color:#123456 }</style>
            <main><section id=left-root><p id=left>left</p></section>
            <aside id=right-root><p id=right>right</p></aside>
            <footer><div>unrelated</div><div>unrelated</div><div>unrelated</div></footer></main>"#,
        "https://example.com/",
    );
    let full = page.refresh_resources(800.0);
    let left_root = element_with_id(&page, "section", "left-root");
    let right_root = element_with_id(&page, "aside", "right-root");
    let left = element_with_id(&page, "p", "left");
    let right = element_with_id(&page, "p", "right");
    left.set_attr("class", "hot");
    right.set_attr("class", "hot");

    let stats = page.refresh_resources_after_invalidation(
        800.0,
        &RenderInvalidation {
            roots: vec![left_root.id(), right_root.id()],
            impact: MutationKind::Attribute("class").impact(),
            mutation_count: 2,
            rebuild_style_rules: false,
            removed_nodes: Vec::new(),
        },
    );

    assert!(!stats.full_rebuild);
    assert!(stats.recomputed_styles < full.total_styles);
    assert_eq!(stats.recomputed_styles, 6);
    let styles = page.cached_style(800.0).unwrap();
    assert_eq!(styles.get(&left).color, Color::rgb(0x12, 0x34, 0x56));
    assert_eq!(styles.get(&right).color, Color::rgb(0x12, 0x34, 0x56));
}

#[test]
fn dynamic_loaded_class_resolves_visibility_inherit_without_stale_hidden_style() {
    let mut page = Page::parse(
        r#"<style>
            .host { visibility: visible }
            .resource { display: inline-block; visibility: hidden; width: 160px; height: 90px }
            .loaded { visibility: inherit }
        </style><div class=host><img id=resource class=resource></div>"#,
        "https://example.com/",
    );
    page.refresh_resources(800.0);
    let resource = element_with_id(&page, "img", "resource");
    assert!(!page.cached_style(800.0).unwrap().get(&resource).visibility);

    resource.set_attr("class", "resource loaded");
    let stats = page.refresh_resources_after_invalidation(
        800.0,
        &RenderInvalidation {
            roots: vec![resource.id()],
            impact: MutationKind::Attribute("class").impact(),
            mutation_count: 1,
            rebuild_style_rules: false,
            removed_nodes: Vec::new(),
        },
    );

    assert_eq!(stats.recomputed_styles, 1);
    assert!(page.cached_style(800.0).unwrap().get(&resource).visibility);
}

#[test]
fn handles_text_insertion_removal_and_viewport_invalidation_conservatively() {
    let mut page = Page::parse(
        "<main id=branch><p>before</p></main><aside>unrelated</aside>",
        "https://example.com/",
    );
    let full = page.refresh_resources(800.0);
    let branch = element_with_id(&page, "main", "branch");
    let paragraph = page.dom.elements_named("p").next().unwrap();
    let text_node = paragraph.children.borrow()[0].clone();
    Node::set_text_content(&text_node, "after");
    let text = page.refresh_resources_after_invalidation(
        800.0,
        &RenderInvalidation {
            roots: vec![branch.id()],
            impact: MutationKind::CharacterData.impact(),
            mutation_count: 1,
            rebuild_style_rules: false,
            removed_nodes: Vec::new(),
        },
    );
    assert_eq!(text.recomputed_styles, 0);
    assert!(text.invalidated_nodes < full.total_styles);

    let inserted = Node::create_element_for(&page.dom.document, "strong");
    assert!(Node::append_child(&branch, inserted.clone()));
    let insertion = page.refresh_resources_after_invalidation(
        800.0,
        &RenderInvalidation {
            roots: vec![branch.id()],
            impact: MutationKind::ChildList.impact(),
            mutation_count: 1,
            rebuild_style_rules: false,
            removed_nodes: Vec::new(),
        },
    );
    assert!(insertion.changed_styles > 0);
    assert!(
        page.cached_style(800.0)
            .unwrap()
            .styles
            .contains_key(&inserted.id())
    );

    assert!(Node::remove_child(&branch, &inserted));
    let removal = page.refresh_resources_after_invalidation(
        800.0,
        &RenderInvalidation {
            roots: vec![branch.id()],
            impact: MutationKind::ChildList.impact(),
            mutation_count: 1,
            rebuild_style_rules: false,
            removed_nodes: vec![inserted.id()],
        },
    );
    assert_eq!(removal.removed_styles, 1);

    let viewport = page.refresh_resources_after_invalidation(
        600.0,
        &RenderInvalidation::viewport(page.dom.document.id()),
    );
    assert!(viewport.full_rebuild);
    assert_eq!(viewport.recomputed_styles, viewport.total_styles);
}

#[test]
fn keeps_style_for_a_node_reinserted_before_the_rendering_checkpoint() {
    let mut page = Page::parse(
        "<main id=branch><p id=target>text</p></main>",
        "https://example.com/",
    );
    page.refresh_resources(800.0);
    let branch = element_with_id(&page, "main", "branch");
    let target = element_with_id(&page, "p", "target");

    assert!(Node::remove_child(&branch, &target));
    assert!(Node::append_child(&branch, target.clone()));
    let stats = page.refresh_resources_after_invalidation(
        800.0,
        &RenderInvalidation {
            roots: vec![branch.id()],
            impact: MutationKind::ChildList.impact(),
            mutation_count: 2,
            rebuild_style_rules: false,
            removed_nodes: vec![target.id()],
        },
    );

    assert_eq!(stats.removed_styles, 0);
    assert!(
        page.cached_style(800.0)
            .unwrap()
            .styles
            .contains_key(&target.id())
    );
}

#[test]
fn rebuilt_layout_does_not_retain_text_or_insertion_geometry() {
    let mut page = Page::parse(
        r#"<style>html,body,main,div{margin:0;padding:0}div{height:20px}
            #first{background:#f00}#last{background:#00f}</style>
            <main id=branch><div id=first>before</div><div id=last>last</div></main>"#,
        "https://example.com/",
    );
    page.refresh_resources(800.0);
    let branch = element_with_id(&page, "main", "branch");
    let last = element_with_id(&page, "div", "last");
    let initial = layout_page(&page, 800.0, 600.0, &mut FixedMeasurer);
    let initial_last_y = solid_rect_y(&initial.items, Color::rgb(0, 0, 255));

    let first = element_with_id(&page, "div", "first");
    let old_text = first.children.borrow()[0].id();
    Node::set_text_content(&first, "after");
    page.refresh_resources_after_invalidation(
        800.0,
        &RenderInvalidation {
            roots: vec![first.id()],
            impact: MutationKind::ChildList.impact(),
            mutation_count: 1,
            rebuild_style_rules: false,
            removed_nodes: vec![old_text],
        },
    );
    let text_layout = layout_page(&page, 800.0, 600.0, &mut FixedMeasurer);
    assert!(
        text_layout
            .items
            .iter()
            .any(|item| matches!(item, DisplayItem::Text { text, .. } if text == "after"))
    );
    assert!(
        !text_layout
            .items
            .iter()
            .any(|item| matches!(item, DisplayItem::Text { text, .. } if text == "before"))
    );

    let inserted = Node::create_element_for(&page.dom.document, "div");
    inserted.set_attr("style", "height:80px;background:#0f0");
    assert!(Node::insert_before(&branch, inserted.clone(), &last));
    page.refresh_resources_after_invalidation(
        800.0,
        &RenderInvalidation {
            roots: vec![branch.id()],
            impact: MutationKind::ChildList.impact(),
            mutation_count: 1,
            rebuild_style_rules: false,
            removed_nodes: Vec::new(),
        },
    );
    let inserted_layout = layout_page(&page, 800.0, 600.0, &mut FixedMeasurer);
    assert!(solid_rect_y(&inserted_layout.items, Color::rgb(0, 0, 255)) > initial_last_y);

    assert!(Node::remove_child(&branch, &inserted));
    page.refresh_resources_after_invalidation(
        800.0,
        &RenderInvalidation {
            roots: vec![branch.id()],
            impact: MutationKind::ChildList.impact(),
            mutation_count: 1,
            rebuild_style_rules: false,
            removed_nodes: vec![inserted.id()],
        },
    );
    let removed_layout = layout_page(&page, 800.0, 600.0, &mut FixedMeasurer);
    assert_eq!(
        solid_rect_y(&removed_layout.items, Color::rgb(0, 0, 255)),
        initial_last_y
    );
}

#[test]
fn stylesheet_text_mutation_forces_rule_rebuild_without_stale_style() {
    let mut page = Page::parse_scripted(
        r#"<style id=theme>p{color:#102030}</style><p id=target>text</p>
            <script>setTimeout(() => {
                document.getElementById('theme').textContent = 'p{color:#abcdef}';
            }, 2000)</script>"#,
        "https://example.com/",
    );
    let mut loader = |_url: &str, _kind: ScriptKind, _options: ScriptFetchOptions| {
        Err("unexpected dynamic script".to_string())
    };
    let (runtime, initial) = page.start_first_paint_script_runtime_with_loader(&mut loader);
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    let mut runtime = runtime.unwrap();
    page.refresh_resources(800.0);

    let outcome = runtime.advance_time(std::time::Duration::from_millis(500), 128);
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert!(outcome.invalidation.rebuild_style_rules);
    assert_eq!(outcome.invalidation.roots, vec![page.dom.document.id()]);
    let stats = page.refresh_resources_after_invalidation(800.0, &outcome.invalidation);

    assert!(stats.full_rebuild);
    let target = element_with_id(&page, "p", "target");
    assert_eq!(
        page.cached_style(800.0).unwrap().get(&target).color,
        Color::rgb(0xab, 0xcd, 0xef)
    );
}

#[test]
fn constructed_stylesheet_replacement_forces_rule_rebuild_without_stale_style() {
    let mut page = Page::parse_scripted(
        r#"<p id=target>text</p><script>
            const sheet = new CSSStyleSheet();
            sheet.replaceSync('#target{color:#102030}');
            document.adoptedStyleSheets = [sheet];
            setTimeout(() => sheet.replaceSync('#target{color:#abcdef}'), 2000);
        </script>"#,
        "https://example.com/",
    );
    let mut loader = |_url: &str, _kind: ScriptKind, _options: ScriptFetchOptions| {
        Err("unexpected dynamic script".to_string())
    };
    let (runtime, initial) = page.start_first_paint_script_runtime_with_loader(&mut loader);
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    let mut runtime = runtime.unwrap();
    page.refresh_resources(800.0);
    let target = element_with_id(&page, "p", "target");
    assert_eq!(
        page.cached_style(800.0).unwrap().get(&target).color,
        Color::rgb(0x10, 0x20, 0x30)
    );

    let outcome = runtime.advance_time(std::time::Duration::from_millis(500), 128);
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert!(outcome.invalidation.rebuild_style_rules);
    assert_eq!(outcome.invalidation.roots, vec![page.dom.document.id()]);
    let stats = page.refresh_resources_after_invalidation(800.0, &outcome.invalidation);

    assert!(stats.full_rebuild);
    assert_eq!(
        page.cached_style(800.0).unwrap().get(&target).color,
        Color::rgb(0xab, 0xcd, 0xef)
    );
}

fn solid_rect_y(items: &[DisplayItem], color: Color) -> f32 {
    items
        .iter()
        .find_map(|item| match item {
            DisplayItem::SolidRect {
                rect,
                color: candidate,
                ..
            } if *candidate == color => Some(rect.y),
            _ => None,
        })
        .unwrap()
}
