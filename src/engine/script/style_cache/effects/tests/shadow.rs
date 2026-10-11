use super::*;
use crate::engine::dom::ShadowRootMode;

#[test]
fn slot_effect_invalidates_assigned_descendants_and_suppressed_fallback() {
    let (dom, mut state) =
        state("<x-card><b>assigned</b></x-card><aside style='color:blue'></aside>");
    let host = dom.elements_named("x-card").next().unwrap();
    let assigned = dom.elements_named("b").next().unwrap();
    let root = Node::attach_shadow(&host, ShadowRootMode::Open, false, false, false).unwrap();
    let slot = Node::create_element_for(&root, "slot");
    let fallback = Node::create_element_for(&root, "i");
    Node::append_child(&slot, fallback.clone());
    Node::append_child(&root, slot.clone());
    slot.set_attr("style", "color:red");
    assert_eq!(color(&mut state, &assigned), "rgb(255, 0, 0)");
    assert_eq!(color(&mut state, &fallback), "rgb(255, 0, 0)");
    sample(&mut state, &slot, "color:green", false);
    assert_eq!(color(&mut state, &assigned), "rgb(0, 128, 0)");
    assert_eq!(color(&mut state, &fallback), "rgb(0, 128, 0)");
}

#[test]
fn host_effect_invalidates_shadow_and_unassigned_light_children() {
    let (dom, mut state) = state("<x-card style='color:red'><b slot='unassigned'></b></x-card>");
    let host = dom.elements_named("x-card").next().unwrap();
    let light = dom.elements_named("b").next().unwrap();
    let root = Node::attach_shadow(&host, ShadowRootMode::Closed, false, false, false).unwrap();
    let shadow = Node::create_element_for(&root, "i");
    Node::append_child(&root, shadow.clone());
    assert_eq!(color(&mut state, &light), "rgb(255, 0, 0)");
    assert_eq!(color(&mut state, &shadow), "rgb(255, 0, 0)");
    sample(&mut state, &host, "color:green", false);
    assert_eq!(color(&mut state, &light), "rgb(0, 128, 0)");
    assert_eq!(color(&mut state, &shadow), "rgb(0, 128, 0)");
}
