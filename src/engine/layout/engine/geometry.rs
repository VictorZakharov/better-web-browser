use super::*;
use crate::engine::invalidation::RenderInvalidation;
use crate::engine::layout::test_support::FixedMeasurer;

fn parse_page(markup: &str) -> Page {
    Page::parse(
        &format!("<style>body{{margin:0}}</style>{markup}"),
        "https://example.test/",
    )
}

fn assert_parity(page: &Page) -> LayoutOutput {
    let retained = layout_page_with_style_viewport(page, 800.0, 600.0, 815.0, &mut FixedMeasurer);
    let geometry = layout_page_for_output(page, 800.0, 600.0, 815.0, &mut FixedMeasurer, false);
    assert_eq!(geometry.node_bounds, retained.node_bounds);
    assert!(
        geometry.items.is_empty(),
        "geometry does not construct paint items"
    );
    assert!(
        geometry.forms.is_empty(),
        "geometry does not collect form actions"
    );
    assert!(
        geometry.node_paint_order.is_empty(),
        "geometry does not collect hit-test order"
    );

    let mut sparse = page.layout_snapshot();
    sparse.refresh_layout_styles_after_invalidation_for_viewport(
        815.0,
        600.0,
        &RenderInvalidation::full(page.dom.document.id()),
    );
    assert_eq!(
        layout_geometry_with_style_viewport(&sparse, 800.0, 600.0, 815.0, &mut FixedMeasurer)
            .node_bounds,
        retained.node_bounds,
        "sparse synchronous styles preserve every retained element box"
    );
    retained
}

#[test]
fn block_inline_and_generated_content_keep_geometry_without_paint_layers() {
    let page = parse_page(
        r#"
        <style>
            main { width:240px; padding:12px; border:2px solid; opacity:.6; overflow:hidden }
            p { margin:0; line-height:28px }
            p::before { content:"prefix "; color:red }
            span { display:inline-block; padding:3px; border:1px solid; transform:translateY(4px) }
            span::after { content:" suffix" }
            em { display:inline-block; width:25px; height:15px; background:blue }
            @media (min-width:810px) { main { width:270px } }
        </style>
        <main><p>wrapping text <span>inside <em></em></span> trailing words across lines</p></main>
    "#,
    );
    let retained = assert_parity(&page);
    let main = page.dom.elements_named("main").next().unwrap();
    let child = page.dom.elements_named("em").next().unwrap();
    assert_eq!(retained.node_bounds[&main.id()].width, 298.0);
    assert!(retained.node_bounds[&child.id()].height > 0.0);
    assert!(
        retained
            .items
            .iter()
            .any(|item| matches!(item, DisplayItem::BeginOpacity { .. }))
    );
    assert!(
        retained
            .items
            .iter()
            .any(|item| matches!(item, DisplayItem::BeginClip { .. }))
    );
}

#[test]
fn flex_grid_and_table_share_complete_box_placement() {
    for markup in [
        r#"<style>
            main { display:flex; flex-wrap:wrap; width:200px; height:100px; align-items:center }
            section { width:80px; padding:2px; transform:translateX(5px) }
            i { display:inline-block; width:20px; height:12px }
        </style><main><section><i></i>A</section><section><i></i>B</section><section>C</section></main>"#,
        r#"<style>
            main { display:flex; flex-direction:column-reverse; width:200px; height:180px; align-items:end }
            section { width:70px; height:30px; background:blue }
            i { display:inline-block; width:20px; height:12px }
        </style><main><section><i></i>A</section><section><i></i>B</section></main>"#,
        r#"<style>
            main { display:grid; width:300px; grid-template-columns:80px 1fr; gap:10px; position:relative }
            section { height:30px; border:1px solid }
            aside { grid-column:1 / 3; height:45px; transform:translateY(7px) }
        </style><main><section>A</section><section>B</section><aside>C</aside></main>"#,
        r#"<style>
            table { width:300px; border:1px solid; border-spacing:4px }
            caption { height:20px } td { padding:3px; border:1px solid }
            .hidden { display:none }
        </style><table><caption>Caption</caption><tbody>
            <tr><td>A</td><td><span>inline cell</span></td><td class=hidden>ignored</td></tr>
            <tr class=hidden><td>hidden row</td></tr><tr><td colspan=2>B</td></tr>
        </tbody></table>"#,
    ] {
        let page = parse_page(markup);
        assert!(!assert_parity(&page).node_bounds.is_empty(), "{markup}");
    }
}

#[test]
fn controls_images_and_svg_preserve_intrinsic_and_percentage_sizing() {
    let mut page = parse_page(
        r#"
        <style>
            form { width:300px; height:260px }
            input, textarea, select, button { margin:3px; padding:2px; border:1px solid }
            .block { display:block; width:50%; height:35px }
            .authored { display:flex; width:120px; height:40px; align-items:center }
            .authored span { display:inline-block; width:20px; height:15px }
            img { width:60px; padding:2px; border:1px solid; transform:translateX(4px) }
        </style>
        <form id=owner action=/submit><input type=hidden name=secret value=one>
            <input value=text><textarea>contents</textarea>
            <select><option>One</option><option selected>Selected</option></select>
            <button>Go</button><input class=block><button class=authored><span></span>Play</button>
            <img src=hero.png><img class=block src=hero.png><img width=12 height=8>
            <svg width=30 height=20><rect width=30 height=20 /></svg>
        </form><input form=owner value=external>
    "#,
    );
    page.images.insert(
        "https://example.test/hero.png".into(),
        crate::engine::DecodedImage {
            width: 3,
            height: 2,
            bgra: vec![255; 24].into(),
        },
    );
    let retained = assert_parity(&page);
    assert!(!retained.forms.is_empty());
    assert!(
        retained
            .items
            .iter()
            .any(|item| matches!(item, DisplayItem::Control(_)))
    );
    for tag in ["input", "textarea", "select", "button", "img", "svg"] {
        assert!(
            page.dom
                .elements_named(tag)
                .any(|node| retained.node_bounds.contains_key(&node.id())),
            "{tag}"
        );
    }
}

#[test]
fn positioned_stacking_transforms_and_clipping_do_not_affect_geometry_only_output() {
    let page = parse_page(
        r#"
        <style>
            main { position:relative; width:250px; height:150px; padding:10px; opacity:.5; overflow:hidden }
            section { position:relative; left:15px; top:8px; width:70px; height:40px; z-index:-2 }
            span { display:inline-block; width:20px; height:10px; transform:translateY(3px) }
            aside { position:absolute; right:5px; bottom:6px; width:60px; height:30px; transform:translate(-10px, -5px); z-index:4 }
            footer { position:fixed; left:30px; top:350px; width:90px; height:25px; z-index:-1 }
        </style>
        <main><section><span>flow</span></section><aside><span>absolute</span></aside></main>
        <footer><span>fixed</span></footer>
    "#,
    );
    let retained = assert_parity(&page);
    let aside = page.dom.elements_named("aside").next().unwrap();
    assert!(retained.node_bounds[&aside.id()].y > 0.0);
    for tag in ["main", "section", "aside", "footer"] {
        let node = page.dom.elements_named(tag).next().unwrap();
        assert!(retained.node_paint_order.contains(&node.id()), "{tag}");
    }
}

#[test]
fn geometry_preserves_shadow_assignment_hidden_roots_and_fullscreen_selection() {
    let page = parse_page(
        "<x-host><main slot=content><span>assigned text</span></main></x-host><p>outside</p>",
    );
    let host = page.dom.elements_named("x-host").next().unwrap();
    let main = page.dom.elements_named("main").next().unwrap();
    let shadow = Node::attach_shadow(
        &host,
        crate::engine::dom::ShadowRootMode::Open,
        false,
        false,
        false,
    )
    .unwrap();
    Node::replace_inner_html(
        &shadow,
        "<style>:host{display:block;width:300px}slot{display:contents}</style><slot name=content></slot>",
        true,
    );
    assert_parity(&page);
    main.set_fullscreen(true);
    let fullscreen = assert_parity(&page);
    // This fixture intentionally gives fullscreen styling a 815px CSS viewport.
    assert_eq!(fullscreen.node_bounds[&main.id()].width, 815.0);
    host.set_attr("style", "display:none");
    assert!(!assert_parity(&page).node_bounds.contains_key(&main.id()));
    for markup in [
        "<body style='display:none'><p>hidden</p>",
        "<html style='display:none'><body><p>hidden</p>",
    ] {
        assert!(assert_parity(&parse_page(markup)).node_bounds.is_empty());
    }
}

#[test]
fn geometry_uses_metrics_without_requesting_painted_glyphs() {
    #[derive(Default)]
    struct Measurer {
        measures: usize,
        shapes: usize,
    }
    impl TextMeasurer for Measurer {
        fn measure(&mut self, text: &str, font: &FontSpec) -> (f32, f32) {
            self.measures += 1;
            FixedMeasurer.measure(text, font)
        }
        fn shape(&mut self, text: &str, font: &FontSpec) -> ShapedText {
            self.shapes += 1;
            let (width, height) = self.measure(text, font);
            ShapedText {
                width,
                height,
                ..ShapedText::default()
            }
        }
    }
    let page = parse_page(
        "<p>Text <span style='display:inline-block;padding:4px'>with nested text</span></p>",
    );
    let mut measurer = Measurer::default();
    let geometry = layout_geometry_with_style_viewport(&page, 800.0, 600.0, 800.0, &mut measurer);
    assert!(measurer.measures > 0);
    assert_eq!(measurer.shapes, 0);
    let retained = layout_page(&page, 800.0, 600.0, &mut measurer);
    assert!(measurer.shapes > 0);
    assert_eq!(geometry.node_bounds, retained.node_bounds);
    assert_eq!(geometry.content_height, retained.content_height);
}
