use super::*;

#[test]
#[ignore = "isolated fixed-overlay reflow timing; run alone with --nocapture"]
fn measure_fixed_overlay_reflow_without_remeasuring_background() {
    let mut page = page("");
    let background = page.dom.elements_named("main").next().unwrap();
    for _ in 0..800 {
        let child = Node::create_element("p");
        Node::append_child(
            &child,
            Node::create_text("background text with styled words"),
        );
        Node::append_child(&background, child);
    }
    page.refresh_resources_for_viewport(800., 600.);
    for round in 0..4 {
        for partial in if round % 2 == 0 {
            [false, true]
        } else {
            [true, false]
        } {
            let mut output = layout_page(&page, 800., 600., &mut FixedMeasurer);
            for sample in 0..15 {
                let invalidation = refresh(
                    &mut page,
                    "bar",
                    if sample % 2 == 0 {
                        "transform:scaleX(.35)"
                    } else {
                        "transform:scaleX(.7)"
                    },
                );
                let start = std::time::Instant::now();
                if partial {
                    assert!(output.try_refresh_fixed_layers(
                        &page,
                        [800., 600., 800.],
                        &invalidation,
                        &mut FixedMeasurer
                    ));
                } else {
                    output = layout_page(&page, 800., 600., &mut FixedMeasurer);
                }
                let elapsed = start.elapsed().as_secs_f64() * 1000.;
                assert_same(
                    &output,
                    &layout_page(&page, 800., 600., &mut FixedMeasurer),
                    &page,
                );
                if sample >= 3 {
                    println!(
                        "fixed-reflow round={round} partial={partial} sample={} ms={elapsed:.4}",
                        sample - 3
                    );
                }
            }
        }
    }
}
