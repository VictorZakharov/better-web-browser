use super::*;
use crate::engine::dom;

mod authored_scope;
mod benchmark;
mod shadow;

fn state(markup: &str) -> (dom::Dom, HostState) {
    let dom = dom::parse(markup);
    let state = HostState::new(
        dom.document.clone(),
        "https://example.com/",
        "UTF-8",
        Rc::new(module_loader::WebModuleLoader::new()),
    );
    (dom, state)
}

fn sample(state: &mut HostState, node: &NodeRef, text: &str, transition: bool) {
    let before = state.document.document_mutation_version();
    assert!(if transition {
        node.set_transition_style(text)
    } else {
        node.set_animation_style(text)
    });
    state.effect_style_changed(node, before);
    state.record_mutation(Some(node), MutationKind::StyleOverlay);
}

fn color(state: &mut HostState, node: &NodeRef) -> String {
    state.computed_style_property(node, "color").unwrap()
}

#[test]
fn effect_sample_preserves_unrelated_cache_and_refreshes_inheritance_and_pseudos() {
    let (dom, mut state) = state(
        "<style>main::before{content:'x';color:inherit}</style>\
         <main style='color:red;--tone:red'><span style='color:var(--tone)'></span></main>\
         <aside style='color:blue'></aside>",
    );
    let target = dom.elements_named("main").next().unwrap();
    let child = dom.elements_named("span").next().unwrap();
    let sibling = dom.elements_named("aside").next().unwrap();
    assert_eq!(color(&mut state, &child), "rgb(255, 0, 0)");
    assert_eq!(color(&mut state, &sibling), "rgb(0, 0, 255)");
    let pseudo = crate::engine::css::PseudoElement::Before;
    assert_eq!(
        state
            .computed_pseudo_style_property(&target, "color", pseudo)
            .unwrap(),
        "rgb(255, 0, 0)"
    );
    let authored = target.attr("style");
    sample(&mut state, &target, "color:green;--tone:green", false);
    let cache = &state.computed_styles.as_ref().unwrap().1.styles;
    assert!(cache.contains_key(&sibling.id()));
    assert!(!cache.contains_key(&target.id()) && !cache.contains_key(&child.id()));
    assert_eq!(color(&mut state, &child), "rgb(0, 128, 0)");
    assert_eq!(
        state
            .computed_pseudo_style_property(&target, "color", pseudo)
            .unwrap(),
        "rgb(0, 128, 0)"
    );
    assert_eq!(target.attr("style"), authored);
    sample(&mut state, &target, "", false);
    assert_eq!(color(&mut state, &child), "rgb(255, 0, 0)");
}

#[test]
fn effect_write_does_not_acknowledge_an_unqueried_author_change() {
    let (dom, mut state) = state("<main><span></span></main><aside style='color:red'></aside>");
    let target = dom.elements_named("main").next().unwrap();
    let sibling = dom.elements_named("aside").next().unwrap();
    assert_eq!(color(&mut state, &sibling), "rgb(255, 0, 0)");
    sibling.set_attr("style", "color:blue");
    state.record_mutation(Some(&sibling), MutationKind::Attribute("style"));
    sample(&mut state, &target, "opacity:.5", false);
    assert_eq!(color(&mut state, &sibling), "rgb(0, 0, 255)");
}

#[test]
fn multiple_or_detached_document_changes_keep_the_full_invalidation_guard() {
    let (dom, mut state) = state("<main></main><aside style='color:red'></aside>");
    let target = dom.elements_named("main").next().unwrap();
    let sibling = dom.elements_named("aside").next().unwrap();
    assert_eq!(color(&mut state, &sibling), "rgb(255, 0, 0)");
    let before = state.document.document_mutation_version();
    target.set_animation_style("opacity:.5");
    sibling.set_attr("style", "color:green");
    state.effect_style_changed(&target, before);
    assert_eq!(color(&mut state, &sibling), "rgb(0, 128, 0)");
    let detached = Node::create_element("div");
    let before = state.document.document_mutation_version();
    detached.set_animation_style("color:blue");
    state.effect_style_changed(&detached, before);
    assert_eq!(color(&mut state, &sibling), "rgb(0, 128, 0)");
}

#[test]
fn root_font_sample_updates_rem_units_in_other_branches() {
    let (dom, mut state) = state(
        "<html style='font-size:10px'><body><main style='width:2rem'></main><aside style='width:3rem'></aside></body></html>",
    );
    let root = dom.elements_named("html").next().unwrap();
    let main = dom.elements_named("main").next().unwrap();
    let sibling = dom.elements_named("aside").next().unwrap();
    assert_eq!(
        state.computed_style_property(&main, "width").unwrap(),
        "20px"
    );
    assert_eq!(
        state.computed_style_property(&sibling, "width").unwrap(),
        "30px"
    );
    sample(&mut state, &root, "font-size:20px", false);
    assert_eq!(
        state.computed_style_property(&sibling, "width").unwrap(),
        "60px"
    );
}

#[test]
fn transition_samples_refresh_the_independent_offset_parent_cache() {
    let (dom, mut state) =
        state("<main><span style='position:fixed'></span></main><aside></aside>");
    let target = dom.elements_named("main").next().unwrap();
    let child = dom.elements_named("span").next().unwrap();
    assert!(state.offset_parent(&child).is_none());
    sample(&mut state, &target, "transform:translateX(2px)", true);
    assert_eq!(state.offset_parent(&child).unwrap().id(), target.id());
    sample(&mut state, &target, "", true);
    assert!(state.offset_parent(&child).is_none());
}

#[test]
fn host_sample_binding_preserves_authored_style_and_relational_selectors() {
    let (dom, mut state) = state(
        "<style>aside{color:blue}body:has(main[style]) aside{color:red}</style><main></main><aside></aside>",
    );
    let target = dom.elements_named("main").next().unwrap();
    let sibling = dom.elements_named("aside").next().unwrap();
    assert_eq!(color(&mut state, &sibling), "rgb(0, 0, 255)");
    let id = state.id_for(&target);
    super::super::super::style_host::style_host_call(
        "setAnimationStyle",
        &[
            JsValue::undefined(),
            JsValue::from(id),
            JsValue::from("color:green".to_owned()),
        ],
        &mut state,
    )
    .unwrap();
    assert_eq!(color(&mut state, &target), "rgb(0, 128, 0)");
    assert!(target.attr("style").is_none());
    assert_eq!(color(&mut state, &sibling), "rgb(0, 0, 255)");
    target.set_attr("style", "opacity:.5");
    state.record_mutation(Some(&target), MutationKind::Attribute("style"));
    assert_eq!(color(&mut state, &sibling), "rgb(255, 0, 0)");
    assert!(
        state
            .pending_invalidation
            .snapshot(0)
            .impact
            .is_style_only()
    );
}
