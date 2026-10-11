use super::*;

struct CountingMeasurer {
    unrelated: usize,
    total: usize,
}
impl TextMeasurer for CountingMeasurer {
    fn measure(&mut self, text: &str, font: &FontSpec) -> (f32, f32) {
        self.total += 1;
        self.unrelated += usize::from(text.contains("background-only"));
        FixedMeasurer.measure(text, font)
    }
}

#[test]
fn retained_reflow_does_not_shape_unrelated_background_text() {
    let mut page = page("");
    let background = page.dom.elements_named("main").next().unwrap();
    for _ in 0..40 {
        let child = Node::create_element("p");
        Node::append_child(
            &child,
            Node::create_text("background-only independent text"),
        );
        Node::append_child(&background, child);
    }
    page.refresh_resources_for_viewport(800., 600.);
    let mut measurer = CountingMeasurer {
        unrelated: 0,
        total: 0,
    };
    let mut retained = layout_page(&page, 800., 600., &mut measurer);
    assert!(measurer.unrelated > 0);
    let invalidation = refresh(&mut page, "bar", "transform:scaleX(.6)");
    measurer.unrelated = 0;
    measurer.total = 0;
    assert!(retained.try_refresh_fixed_layers(
        &page,
        [800., 600., 800.],
        &invalidation,
        &mut measurer
    ));
    assert_eq!(measurer.unrelated, 0);
    assert!(
        measurer.total > 0,
        "the dirty overlay still uses ordinary text layout"
    );
    assert_same(
        &retained,
        &layout_page(&page, 800., 600., &mut FixedMeasurer),
        &page,
    );
}

#[test]
fn reflowed_text_lines_do_not_alias_retained_background_lines() {
    let mut page = page("");
    let mut output = layout_page(&page, 800., 600., &mut FixedMeasurer);
    let old_next = output.fragments.next_line;
    let invalidation = refresh(&mut page, "title", "width:70px;font-size:18px");
    assert!(output.try_refresh_fixed_layers(
        &page,
        [800., 600., 800.],
        &invalidation,
        &mut FixedMeasurer
    ));
    let members = &output.fixed_layers.layers[&node(&page, "overlay").id()].members;
    for (&id, fragments) in &output.fragments.text {
        for fragment in fragments {
            assert_eq!(fragment.line >= old_next, members.contains(&id));
        }
    }
    assert_same(
        &output,
        &layout_page(&page, 800., 600., &mut FixedMeasurer),
        &page,
    );
}
