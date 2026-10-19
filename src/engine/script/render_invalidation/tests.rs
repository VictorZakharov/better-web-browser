use super::*;
use crate::engine::dom;

#[test]
fn native_effects_use_target_roots_but_author_writes_keep_sibling_scope() {
    let dom = dom::parse("<main><b></b><i></i></main>");
    let parent = dom.elements_named("main").next().unwrap();
    let target = dom.elements_named("b").next().unwrap();
    let mut pending = PendingInvalidation::default();
    pending.record(&dom.document, Some(&target), MutationKind::StyleOverlay);
    assert_eq!(pending.snapshot(1).roots, vec![target.id()]);
    assert!(pending.snapshot(1).impact.affects_layout());
    pending.record(
        &dom.document,
        Some(&target),
        MutationKind::Attribute("style"),
    );
    assert_eq!(pending.snapshot(2).roots, vec![parent.id()]);
}

#[test]
fn validation_mutations_widen_roots_to_form_and_fieldsets() {
    let document =
        dom::parse("<form><fieldset><div><input required></div></fieldset></form><p></p>");
    let input = document.elements_named("input").next().unwrap();
    let form = document.elements_named("form").next().unwrap();
    let mut pending = PendingInvalidation::default();
    pending.record(
        &document.document,
        Some(&input),
        MutationKind::Attribute("required"),
    );
    // The form root covers the nested fieldset and control subtrees.
    let roots = pending.snapshot(1).roots;
    assert!(roots.contains(&form.id()));
    assert!(!roots.contains(&document.document.id()));

    // A standalone fieldset (no form owner) becomes a root of its own.
    let grouped = dom::parse("<fieldset><div><input required></div></fieldset>");
    let lone_input = grouped.elements_named("input").next().unwrap();
    let lone_set = grouped.elements_named("fieldset").next().unwrap();
    let mut grouped_pending = PendingInvalidation::default();
    grouped_pending.record(
        &grouped.document,
        Some(&lone_input),
        MutationKind::Attribute("required"),
    );
    assert!(grouped_pending.snapshot(1).roots.contains(&lone_set.id()));

    // Unrelated attributes keep their existing narrow roots.
    let mut narrow = PendingInvalidation::default();
    narrow.record(
        &document.document,
        Some(&input),
        MutationKind::Attribute("class"),
    );
    let roots = narrow.snapshot(1).roots;
    assert!(!roots.contains(&form.id()));

    // Ownerless controls add no aggregation roots at all.
    let bare = dom::parse("<div><input required></div>");
    let lone = bare.elements_named("input").next().unwrap();
    let mut lone_pending = PendingInvalidation::default();
    lone_pending.record(&bare.document, Some(&lone), MutationKind::State);
    assert_eq!(lone_pending.snapshot(1).roots.len(), 1);
}

#[test]
fn published_geometry_preserves_pending_style_and_removal_obligations() {
    let dom = dom::parse("<style>p{color:red}</style><main><p>text</p></main>");
    let sheet = dom.elements_named("style").next().unwrap();
    let paragraph = dom.elements_named("p").next().unwrap();
    let mut pending = PendingInvalidation::default();
    pending.record(&dom.document, Some(&sheet), MutationKind::Stylesheet);
    pending.record_removed_subtree(&paragraph);
    let before = pending.snapshot(2);

    pending.acknowledge_published_geometry();
    let after = pending.snapshot(2);
    assert!(before.impact.affects_intrinsic_size());
    assert!(!after.impact.affects_intrinsic_size());
    assert!(after.impact.affects_style() && after.impact.affects_layout());
    assert!(after.impact.affects_paint() && after.rebuild_style_rules);
    assert_eq!(after.roots, before.roots);
    assert_eq!(after.removed_nodes, before.removed_nodes);
    assert_eq!(after.mutation_count, before.mutation_count);

    pending.record(&dom.document, Some(&paragraph), MutationKind::CharacterData);
    assert!(pending.snapshot(3).impact.affects_intrinsic_size());
}

#[test]
fn child_list_parents_stay_bounded_until_a_full_root_covers_them() {
    let dom = dom::parse("<main><section><div><p></p></div></section></main>");
    let main = dom.elements_named("main").next().unwrap();
    let section = dom.elements_named("section").next().unwrap();
    let paragraph = dom.elements_named("p").next().unwrap();
    let mut pending = PendingInvalidation::default();
    pending.record(&dom.document, Some(&section), MutationKind::ChildList);
    let snapshot = pending.snapshot(1);
    assert_eq!(snapshot.roots, vec![section.id()]);
    assert_eq!(snapshot.child_list_roots, vec![section.id()]);

    // A child-list parent never covers a full root below it.
    pending.record(
        &dom.document,
        Some(&paragraph),
        MutationKind::Attribute("style"),
    );
    assert_eq!(pending.snapshot(2).child_list_roots, vec![section.id()]);

    // A full root above it restyles the whole subtree anyway.
    pending.record(
        &dom.document,
        Some(&section),
        MutationKind::Attribute("style"),
    );
    let snapshot = pending.snapshot(3);
    assert_eq!(snapshot.roots, vec![main.id()]);
    assert!(snapshot.child_list_roots.is_empty());
}

#[test]
fn coalesces_sibling_mutations_at_their_parent() {
    let document = dom::parse("<main><p id=a></p><p id=b></p></main>");
    let main = document.elements_named("main").next().unwrap();
    let children = main.children.borrow().clone();
    let mut pending = PendingInvalidation::default();
    pending.record(
        &document.document,
        Some(&children[0]),
        MutationKind::Attribute("class"),
    );
    pending.record(
        &document.document,
        Some(&children[1]),
        MutationKind::Attribute("hidden"),
    );

    let invalidation = pending.snapshot(2);
    assert_eq!(invalidation.roots, vec![main.id()]);
    assert_eq!(invalidation.mutation_count, 2);
}

#[test]
fn removed_subtrees_retain_nonlocal_dependencies_until_consumed() {
    for (source, local) in [
        ("<section><script></script></section>", true),
        ("<section><svg><defs/></svg></section>", false),
        ("<section><base href='/other/'></section>", false),
        ("<section><slot></slot></section>", false),
    ] {
        let dom = dom::parse(source);
        let root = dom.elements_named("section").next().unwrap();
        let mut pending = PendingInvalidation::default();
        pending.record_removed_subtree(&root);
        pending.acknowledge_published_geometry();
        assert_eq!(pending.take(1).removals_are_local, local, "{source}");
        assert!(!pending.snapshot(0).removals_are_local);
    }
    let dom = dom::parse("<main><p>fullscreen</p></main>");
    let root = dom.elements_named("main").next().unwrap();
    dom.elements_named("p").next().unwrap().set_fullscreen(true);
    let mut pending = PendingInvalidation::default();
    pending.record_removed_subtree(&root);
    assert!(!pending.snapshot(1).removals_are_local);
}

#[test]
fn retains_disjoint_component_roots() {
    let document = dom::parse("<main><section><p></p></section><aside><p></p></aside></main>");
    let section = document.elements_named("section").next().unwrap();
    let aside = document.elements_named("aside").next().unwrap();
    let left = section.children.borrow()[0].clone();
    let right = aside.children.borrow()[0].clone();
    let mut pending = PendingInvalidation::default();
    pending.record(
        &document.document,
        Some(&left),
        MutationKind::Attribute("class"),
    );
    pending.record(
        &document.document,
        Some(&right),
        MutationKind::Attribute("class"),
    );

    let mut roots = pending.snapshot(2).roots;
    roots.sort_unstable();
    let mut expected = vec![section.id(), aside.id()];
    expected.sort_unstable();
    assert_eq!(roots, expected);
}
