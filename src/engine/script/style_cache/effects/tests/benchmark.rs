use super::*;

#[test]
#[ignore = "isolated effect/cache timing; run alone with --nocapture"]
fn measure_effect_samples_without_discarding_unrelated_styles() {
    let css = (0..100)
        .map(|n| format!(".layer-{n}{{--v-{n}:{n}px;margin-left:{n}px}}"))
        .collect::<String>();
    let classes = (0..100).map(|n| format!("layer-{n} ")).collect::<String>();
    let markup = format!(
        "<style>{css}</style><main class='{classes}' style='color:red'>{}</main>",
        "<div><span></span></div>".repeat(24)
    );
    let (dom, mut state) = state(&markup);
    let nodes = dom.elements_named("div").collect::<Vec<_>>();
    let children = dom.elements_named("span").collect::<Vec<_>>();
    for round in 0..4 {
        for local in if round % 2 == 0 {
            [false, true]
        } else {
            [true, false]
        } {
            for sample in 0..15 {
                state.computed_styles = None;
                let start = std::time::Instant::now();
                let mut values = Vec::new();
                for frame in 0..8 {
                    for (node, child) in nodes.iter().zip(&children) {
                        let before = state.document.document_mutation_version();
                        node.set_animation_style(if frame % 2 == 0 {
                            "color:green"
                        } else {
                            "color:blue"
                        });
                        if local {
                            state.effect_style_changed(node, before);
                        }
                        values.push(color(&mut state, child));
                    }
                }
                let elapsed = start.elapsed().as_secs_f64() * 1000.0;
                for (frame, values) in values.chunks(nodes.len()).enumerate() {
                    let expected = if frame % 2 == 0 {
                        "rgb(0, 128, 0)"
                    } else {
                        "rgb(0, 0, 255)"
                    };
                    assert!(values.iter().all(|value| value == expected));
                }
                if sample >= 3 {
                    println!(
                        "effect-cache round={round} local={local} sample={} ms={elapsed:.4}",
                        sample - 3
                    );
                }
            }
        }
    }
}
