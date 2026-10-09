use super::*;

fn host(source: &str) -> HostState {
    let dom = crate::engine::dom::parse(source);
    HostState::new(
        dom.document,
        "https://example.test/",
        "UTF-8",
        Rc::new(module_loader::WebModuleLoader::new()),
    )
}

fn snapshot_len(state: &mut HostState) -> usize {
    let JsValue::Array(snapshot) = state.css_animation_snapshot() else {
        panic!("animation snapshot array");
    };
    snapshot.len()
}

#[test]
fn large_static_branch_is_not_cascaded_by_a_descendant_reset_rule() {
    let markup = format!(
        "<style>@keyframes fade{{to{{opacity:.5}}}} .dialog *{{animation:none}} .active{{animation:fade 1s}}</style><main>{}</main><aside class=dialog><i></i></aside><div class=active></div>",
        "<p>static text</p>".repeat(2000)
    );
    let mut state = host(&markup);
    assert_eq!(snapshot_len(&mut state), 1);
    let computed = &state.computed_styles.as_ref().unwrap().1.styles;
    assert!(
        computed.len() < 20,
        "discovery cascaded {} nodes",
        computed.len()
    );
    let main = Node::descendants(&state.document)
        .find(|node| node.tag_name() == Some("main"))
        .unwrap();
    for index in 0..5 {
        main.set_attr("data-update", &index.to_string());
        state.record_mutation(Some(&main), MutationKind::Attribute("data-update"));
        assert_eq!(snapshot_len(&mut state), 1);
        assert!(state.computed_styles.as_ref().unwrap().1.styles.len() < 20);
    }
}

#[test]
fn ancestor_changes_start_and_remove_candidates_without_a_frame_delay() {
    let mut state = host(
        "<style>@keyframes fade{to{opacity:.5}} .active > p{animation:fade 1s} .disabled{display:none}</style><main><p></p></main>",
    );
    let main = Node::descendants(&state.document)
        .find(|node| node.tag_name() == Some("main"))
        .unwrap();
    assert_eq!(snapshot_len(&mut state), 0);
    for (classes, count) in [
        ("active", 1),
        ("active disabled", 0),
        ("active", 1),
        ("", 0),
    ] {
        main.set_attr("class", classes);
        state.record_mutation(Some(&main), MutationKind::Attribute("class"));
        assert_eq!(snapshot_len(&mut state), count, "{classes}");
    }
}
