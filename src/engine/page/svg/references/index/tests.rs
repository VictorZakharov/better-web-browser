use super::*;
use crate::engine::page::{Page, svg::serialize};

fn page() -> Page {
    Page::parse(
        r##"<svg><defs><rect id=shape width=10 height=10 fill=red /></defs></svg>
        <svg width=10 height=10><use href="#shape"/></svg>
        <svg width=10 height=10><defs><rect id=shape width=10 height=10 fill=blue /></defs><use href="#shape"/></svg>"##,
        "https://example.test/",
    )
}

#[test]
fn batch_serialization_is_identical_and_first_duplicate_id_is_not_replaced() {
    let page = page();
    let mut cache = IndexCache::default();
    for svg in page.dom.elements_named("svg") {
        assert_eq!(
            serialize::source(&svg, None).unwrap(),
            serialize::source_with_index(&svg, None, &mut cache).unwrap()
        );
    }
    assert_eq!(cache.builds, 1);
    assert_eq!(cache.trees.len(), 1);
    let first = page.dom.elements_named("rect").next().unwrap();
    let index = cache.get(&first).unwrap();
    assert!(index.retain_id("shape", first.id()));
    assert!(!index.retain_id(
        "shape",
        page.dom.elements_named("rect").last().unwrap().id()
    ));
}

#[test]
fn new_batches_observe_definition_mutation_removal_and_new_first_matches() {
    let page = page();
    let svg = page.dom.elements_named("svg").last().unwrap();
    let before = serialize::source_with_index(&svg, None, &mut IndexCache::default()).unwrap();
    let first = page.dom.elements_named("rect").next().unwrap();
    first.set_attr("fill", "green");
    let after = serialize::source_with_index(&svg, None, &mut IndexCache::default()).unwrap();
    assert_ne!(before, after);
    assert!(after.contains("fill=\"green\""));
    first.remove_attr("id");
    let after = serialize::source_with_index(&svg, None, &mut IndexCache::default()).unwrap();
    assert!(after.contains("fill=\"blue\""));
    assert!(!after.contains("fill=\"green\""));
}

#[test]
fn independent_shadow_trees_do_not_share_definition_names() {
    use crate::engine::dom::ShadowRootMode;
    let page = Page::parse(
        r##"<svg><defs><rect id=shape width=10 height=10 fill=red /></defs></svg>
        <div></div><svg width=10 height=10><use href="#shape"/></svg>"##,
        "https://example.test/",
    );
    let shadow_svg = page.dom.elements_named("svg").last().unwrap();
    let host = page.dom.elements_named("div").next().unwrap();
    let shadow = Node::attach_shadow(&host, ShadowRootMode::Closed, false, false, false).unwrap();
    Node::append_child(&shadow, shadow_svg.clone());
    let document_svg = page.dom.elements_named("svg").next().unwrap();
    let mut cache = IndexCache::default();
    let document = cache.get(&document_svg).unwrap();
    let isolated = cache.get(&shadow_svg).unwrap();
    assert!(document.ids.contains_key("shape"));
    assert!(!isolated.ids.contains_key("shape"));
    assert!(!Rc::ptr_eq(&document, &isolated));
    assert_eq!(cache.builds, 2);
    assert_eq!(
        serialize::source(&shadow_svg, None).unwrap(),
        serialize::source_with_index(&shadow_svg, None, &mut cache).unwrap()
    );
}

#[test]
fn optional_metadata_exhaustion_keeps_valid_dependency_resolution() {
    for (nodes, key_bytes) in [(MAX_DOM_NODES, 0), (0, MAX_SVG_SOURCE_BYTES)] {
        let page = page();
        let svg = page.dom.elements_named("svg").last().unwrap();
        let mut cache = IndexCache {
            nodes,
            key_bytes,
            ..Default::default()
        };
        let expected = serialize::source(&svg, None).unwrap();
        for _ in 0..2 {
            assert_eq!(
                serialize::source_with_index(&svg, None, &mut cache).unwrap(),
                expected
            );
        }
        assert_eq!(cache.builds, 2);
        assert!(cache.trees.is_empty());
        assert_eq!((cache.nodes, cache.key_bytes), (nodes, key_bytes));
    }
}

#[test]
fn reference_free_svg_does_not_build_an_unused_document_index() {
    let page = Page::parse(
        "<svg width=10 height=10><rect width=10 height=10 fill=red /></svg>",
        "https://example.test/",
    );
    let svg = page.dom.elements_named("svg").next().unwrap();
    let mut cache = IndexCache::default();
    serialize::source_with_index(&svg, None, &mut cache).unwrap();
    assert_eq!(cache.builds, 0);
}

#[test]
#[ignore = "isolated SVG serialization command; run alone without heavy builds or suites"]
fn benchmark_repeated_svg_reference_resolution() {
    let html = format!("{}{}", (0..8000).map(|n| format!("<span id=decoration-{n}></span>")).collect::<String>(),
        r##"<svg width=10 height=10><defs><rect id=shape width=10 height=10 fill=red /></defs><use href="#shape"/></svg>"##.repeat(32));
    let page = Page::parse(&html, "https://example.test/");
    let svgs = page.dom.elements_named("svg").collect::<Vec<_>>();
    let expected = svgs
        .iter()
        .map(|svg| serialize::source(svg, None).unwrap())
        .collect::<Vec<_>>();
    for round in 0..4 {
        for shared in if round % 2 == 0 {
            [false, true]
        } else {
            [true, false]
        } {
            let mut samples = Vec::new();
            for sample in 0..15 {
                let mut cache = IndexCache::default();
                let start = std::time::Instant::now();
                let result = svgs
                    .iter()
                    .map(|svg| {
                        if shared {
                            serialize::source_with_index(svg, None, &mut cache).unwrap()
                        } else {
                            serialize::source(svg, None).unwrap()
                        }
                    })
                    .collect::<Vec<_>>();
                let elapsed = start.elapsed().as_secs_f64() * 1000.0;
                assert_eq!(result, expected, "changed decoder input");
                if sample >= 3 {
                    samples.push(elapsed);
                }
                if shared {
                    assert_eq!(cache.builds, 1);
                }
            }
            println!("round={round} shared={shared} completed_32_svg_sources_ms={samples:?}");
        }
    }
}
