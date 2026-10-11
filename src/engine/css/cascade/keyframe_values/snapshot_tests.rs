//! Immutable inputs live only for one synchronous discovery operation.
use super::*;

fn declarations(values: &[(&str, &str)]) -> Vec<(String, String)> {
    values
        .iter()
        .map(|(name, value)| (name.to_string(), value.to_string()))
        .collect()
}

#[test]
fn shared_snapshot_preserves_independent_blocks_parent_effects_and_author_inputs() {
    let dom = dom::parse(
        "<style>main{font-size:20px;color:blue}div{color:red;--end:.8}</style><main><div></div></main>",
    );
    let parent = dom.elements_named("main").next().unwrap();
    let target = dom.elements_named("div").next().unwrap();
    parent.set_animation_style("color:green");
    target.set_animation_style("color:yellow;opacity:.4;font-size:30px");
    let mut styles = StyleSet::from_dom(&dom, &[], 800.0);
    let snapshot = styles.keyframe_values_for_node(&target);
    let blocks = [
        declarations(&[
            ("font-size", "40px"),
            ("width", "2em"),
            ("opacity", "var(--end)"),
        ]),
        declarations(&[
            ("color", "inherit"),
            ("width", "2em"),
            ("opacity", "revert"),
        ]),
        declarations(&[("margin", "1em 2px"), ("font-size", "10px")]),
        declarations(&[("width", "2em"), ("opacity", "var(--missing,.5)")]),
    ];
    for block in &blocks {
        assert_eq!(
            snapshot.resolve(block),
            styles.resolved_keyframe_values(&target, block)
        );
    }
    assert!(
        snapshot
            .resolve(&blocks[0])
            .contains(&("width".into(), "80px".into()))
    );
    let inherited = snapshot.resolve(&blocks[1]);
    assert!(inherited.contains(&("color".into(), "rgb(0, 128, 0)".into())));
    assert!(inherited.contains(&("width".into(), "40px".into())));
    assert!(inherited.contains(&("opacity".into(), "1".into())));
    assert_eq!(
        target.animation_style().as_deref(),
        Some("color:yellow;opacity:.4;font-size:30px")
    );
    assert!(target.attr("style").is_none());
}

#[test]
fn later_snapshots_observe_variables_parent_effects_and_relative_units() {
    let dom = dom::parse("<main><div style='--end:.8;font-size:20px'></div></main>");
    let parent = dom.elements_named("main").next().unwrap();
    let target = dom.elements_named("div").next().unwrap();
    let mut styles = StyleSet::from_dom(&dom, &[], 800.0);
    let block = declarations(&[
        ("opacity", "var(--end)"),
        ("width", "2em"),
        ("color", "inherit"),
    ]);
    let before = styles.keyframe_values_for_node(&target).resolve(&block);
    target.set_attr("style", "--end:.3;font-size:40px");
    parent.set_animation_style("color:blue");
    styles.clear_computed_styles();
    let after = styles.keyframe_values_for_node(&target).resolve(&block);
    assert_ne!(before, after);
    assert!(after.contains(&("opacity".into(), "0.3".into())));
    assert!(after.contains(&("width".into(), "80px".into())));
    assert!(after.contains(&("color".into(), "rgb(0, 0, 255)".into())));
}

#[test]
#[ignore = "isolated keyframe resolution timing; run alone with --nocapture"]
fn measure_keyframe_blocks_with_shared_target_inputs() {
    let css = (0..160)
        .map(|n| format!(".layer-{n}{{margin-left:{n}px;--v-{n}:{n}px}} "))
        .collect::<String>();
    let classes = (0..160).map(|n| format!("layer-{n} ")).collect::<String>();
    let markup = format!(
        "<style>{css}</style><main style='font-size:20px;--end:.8'>{}</main>",
        format!("<div class='{classes}'></div>").repeat(32)
    );
    let dom = dom::parse(&markup);
    let nodes = dom.elements_named("div").collect::<Vec<_>>();
    let mut styles = StyleSet::from_dom(&dom, &[], 800.0);
    let blocks = (0..12)
        .map(|n| {
            declarations(&[
                ("opacity", "var(--end)"),
                ("width", "2em"),
                ("margin", if n % 2 == 0 { "1em 2px" } else { "3px 2em" }),
            ])
        })
        .collect::<Vec<_>>();
    let expected = nodes
        .iter()
        .map(|node| {
            blocks
                .iter()
                .map(|block| styles.resolved_keyframe_values(node, block))
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    for round in 0..4 {
        for shared in if round % 2 == 0 {
            [false, true]
        } else {
            [true, false]
        } {
            for sample in 0..15 {
                styles.clear_computed_styles();
                let start = std::time::Instant::now();
                let actual = nodes
                    .iter()
                    .map(|node| {
                        if shared {
                            let values = styles.keyframe_values_for_node(node);
                            blocks
                                .iter()
                                .map(|block| values.resolve(block))
                                .collect::<Vec<_>>()
                        } else {
                            blocks
                                .iter()
                                .map(|block| styles.resolved_keyframe_values(node, block))
                                .collect::<Vec<_>>()
                        }
                    })
                    .collect::<Vec<_>>();
                let elapsed = start.elapsed().as_secs_f64() * 1000.0;
                assert_eq!(actual, expected);
                if sample >= 3 {
                    println!(
                        "keyframe-inputs round={round} shared={shared} sample={} ms={elapsed:.4}",
                        sample - 3
                    );
                }
            }
        }
    }
}
