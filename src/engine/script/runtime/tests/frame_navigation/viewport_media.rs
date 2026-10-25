use super::*;
use crate::engine::RectF;
use std::collections::HashMap;

#[test]
fn child_media_queries_follow_a_script_resized_iframe_content_box_synchronously() {
    // A nested document's viewport is its iframe's content box (borders excluded).
    // https://drafts.csswg.org/mediaqueries-4/#mf-dimensions
    let (dom, mut runtime) = start(
        r#"<body><style>iframe { border: 2px solid; padding: 0 }</style><script>
        window.f = document.createElement('iframe');
        f.srcdoc = '<style>@media (min-width: 50px) { p { text-transform: uppercase } }</style><p id="p"></p>';
        document.body.append(f);
        </script>"#,
    );
    drain(&mut runtime);
    let iframe = dom.elements_named("iframe").next().unwrap();
    let id = iframe.id();
    runtime.set_layout_flush_callback(Box::new(move |_, _| {
        let wide = iframe
            .attr("style")
            .is_some_and(|style| style.contains("100px"));
        let width = if wide { 104.0 } else { 4.0 };
        let rect = RectF {
            width,
            height: 4.0,
            ..RectF::default()
        };
        Some(HashMap::from([(id, rect)]))
    }));
    evaluate(
        &mut runtime,
        &dom,
        r#"
        const read = () => f.contentWindow.getComputedStyle(
            f.contentDocument.getElementById('p')).textTransform;
        const states = [read()];
        f.style.width = '100px';
        states.push(read());
        f.removeAttribute('style');
        states.push(read());
        if (states.join() !== 'none,uppercase,none') throw Error(states.join());
        "#,
    );
}

struct FixedMeasurer;

impl crate::engine::TextMeasurer for FixedMeasurer {
    fn measure(&mut self, text: &str, font: &crate::engine::FontSpec) -> (f32, f32) {
        (text.chars().count() as f32 * font.size * 0.5, font.size)
    }
}

#[test]
fn zero_size_child_document_lays_out_on_demand_for_rendered_image_dimensions() {
    // The child of a zero-size iframe is never composed into paint, but it is still rendered:
    // its image reports the used CSS height, which follows later style text changes.
    // https://html.spec.whatwg.org/multipage/embedded-content.html#dom-img-height
    let (dom, mut runtime) = start(
        r#"<body><style>iframe { border: 0; width: 0; height: 0 }</style><script>
        window.f = document.createElement('iframe');
        f.srcdoc = '<style>img { height: 10px }</style><img alt="">';
        document.body.append(f);
        </script>"#,
    );
    drain(&mut runtime);
    let iframe = dom.elements_named("iframe").next().unwrap().id();
    runtime.set_layout_flush_callback(Box::new(move |_, _| {
        Some(HashMap::from([(iframe, RectF::default())]))
    }));
    let layouts = std::rc::Rc::new(std::cell::Cell::new(0));
    let counter = std::rc::Rc::clone(&layouts);
    runtime.set_frame_layout_service(std::rc::Rc::new(move |page, width, height, hit_test| {
        counter.set(counter.get() + 1);
        if hit_test {
            return Some(crate::engine::layout_page_with_style_viewport(
                page,
                width,
                height,
                width,
                &mut FixedMeasurer,
            ));
        }
        Some(crate::engine::layout_geometry_with_style_viewport(
            page,
            width,
            height,
            width,
            &mut FixedMeasurer,
        ))
    }));
    evaluate(
        &mut runtime,
        &dom,
        r#"
        const doc = f.contentDocument;
        const heights = [doc.images[0].height];
        doc.querySelector('style').firstChild.data = 'img { height: 20px }';
        heights.push(doc.images[0].height);
        if (heights.join() !== '10,20') throw Error(heights.join());
        "#,
    );
    assert_eq!(layouts.get(), 2, "one layout per mutation, not per read");
    // Unchanged documents reuse the layout for repeated geometry and hit-test reads.
    evaluate(
        &mut runtime,
        &dom,
        r#"
        const image = f.contentDocument.images[0];
        image.height; image.width; image.height;
        f.contentDocument.elementFromPoint(1, 1); f.contentDocument.elementFromPoint(1, 1);
        "#,
    );
    assert_eq!(
        layouts.get(),
        3,
        "one extra layout for the first hit test only"
    );
}
