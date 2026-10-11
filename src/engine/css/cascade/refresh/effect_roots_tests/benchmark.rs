use super::*;

#[test]
#[ignore = "isolated effect subtree refresh timing; run alone with --nocapture"]
fn measure_effect_target_refresh_in_a_large_sibling_set() {
    let css = (0..100)
        .map(|n| format!(".layer-{n}{{--v-{n}:{n}px}}"))
        .collect::<String>();
    let classes = (0..100).map(|n| format!("layer-{n} ")).collect::<String>();
    let dom = dom::parse(&format!(
        "<style>{css}</style><main class='{classes}'>{}</main>",
        "<div><span>text</span></div>".repeat(800)
    ));
    let parent = dom.elements_named("main").next().unwrap();
    let target = dom.elements_named("div").next().unwrap();
    let mut styles = StyleSet::from_dom(&dom, &[], 800.0);
    for round in 0..4 {
        for narrow in if round % 2 == 0 {
            [false, true]
        } else {
            [true, false]
        } {
            for sample in 0..15 {
                target.set_animation_style(if sample % 2 == 0 {
                    "color:green;transform:scale(.8)"
                } else {
                    "color:blue;transform:scale(.9)"
                });
                let start = std::time::Instant::now();
                styles.refresh_subtrees_after_invalidation(
                    &dom.document,
                    std::slice::from_ref(if narrow { &target } else { &parent }),
                    &[],
                    MutationKind::StyleOverlay.impact(),
                );
                let elapsed = start.elapsed().as_secs_f64() * 1000.0;
                assert_eq!(styles.styles, StyleSet::from_dom(&dom, &[], 800.0).styles);
                if sample >= 3 {
                    println!(
                        "effect-roots round={round} narrow={narrow} sample={} ms={elapsed:.4}",
                        sample - 3
                    );
                }
            }
        }
    }
}
