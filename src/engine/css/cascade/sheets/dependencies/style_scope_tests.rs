use super::*;

fn styles(css: &str) -> StyleSet {
    StyleSet::from_dom(
        &dom::parse(&format!(
            "<style>{css}</style><main><b></b></main><aside></aside>"
        )),
        &[],
        800.0,
    )
}

#[test]
fn ordinary_rules_without_style_attribute_inputs_allow_target_scope() {
    for selector in [
        "main",
        "main + aside",
        "main:has(b)",
        ":is(main,aside)",
        "main:nth-child(1 of .a)",
        "[title=style]",
        ":not([class])",
    ] {
        assert!(
            styles(&format!("{selector}{{color:red}}")).can_scope_inline_style_to_target(),
            "{selector}"
        );
    }
}

#[test]
fn every_style_attribute_selector_graph_declines_target_scope() {
    for selector in [
        "[style]",
        "[STYLE]",
        "main[style] + aside",
        "main:has([style]) ~ aside",
        ":is([class],:not([style]))",
        "main:nth-child(1 of [style])",
        "main[style*=opacity]",
        "[style|=none]",
    ] {
        assert!(
            !styles(&format!("{selector}{{color:red}}")).can_scope_inline_style_to_target(),
            "{selector}"
        );
    }
}

#[test]
fn scoped_and_cross_tree_selector_graphs_remain_conservative() {
    assert!(!styles("@scope(main){b{color:red}}").can_scope_inline_style_to_target());
    let dom = dom::parse("<x-card></x-card>");
    let host = dom.elements_named("x-card").next().unwrap();
    let root = Node::attach_shadow(&host, dom::ShadowRootMode::Open, false, false, false).unwrap();
    let sheet = Node::create_element_for(&root, "style");
    Node::append_child(
        &sheet,
        Node::create_text_for(&root, ":host{color:red}slot::slotted(b){color:blue}"),
    );
    Node::append_child(&root, sheet);
    assert!(!StyleSet::from_dom(&dom, &[], 800.0).can_scope_inline_style_to_target());
}
