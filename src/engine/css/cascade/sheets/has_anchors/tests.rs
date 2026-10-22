//! Bounded `:has()` refreshes must equal a whole-document cascade.
use crate::engine::css::cascade::StyleSet;
use crate::engine::css::media::MediaEnvironment;
use crate::engine::dom::{self, Dom, Node, NodeRef};

fn environment() -> MediaEnvironment {
    MediaEnvironment::new(800.0, 600.0, 1.0, false)
}

fn by_id(dom: &Dom, id: &str) -> NodeRef {
    Node::descendants(&dom.document)
        .find(|node| node.attr("id").as_deref() == Some(id))
        .unwrap_or_else(|| panic!("#{id}"))
}

/// Applies `mutate`, refreshes from the root ordinary invalidation records, and compares every
/// computed style with a fresh cascade. Returns (recomputed, total) styles.
fn refresh_matches_full_cascade(
    css: &str,
    body: &str,
    mutate: impl FnOnce(&Dom) -> NodeRef,
) -> (usize, usize) {
    let dom = dom::parse(&format!("<style>{css}</style><body>{body}</body>"));
    let mut styles = StyleSet::from_sources_for_layout(&dom, "", &[], environment());
    let root = mutate(&dom);
    let stats = styles.refresh_subtrees(&dom.document, &[root], &[]);
    let fresh = StyleSet::from_sources_for_layout(&dom, "", &[], environment());
    for node in Node::descendants(&dom.document) {
        assert_eq!(
            styles.styles.get(&node.id()),
            fresh.styles.get(&node.id()),
            "{css} :: <{}> {:?}",
            node.tag_name().unwrap_or_default(),
            node.attr("id")
        );
    }
    (stats.recomputed_styles, fresh.styles.len())
}

// Attribute writes record the target's parent; child-list changes record the parent.
fn add_class(dom: &Dom, id: &str, class: &str) -> NodeRef {
    let target = by_id(dom, id);
    target.set_attr("class", class);
    target.parent().unwrap()
}

const LIST: &str = "<main><ul id=list><li id=first><div><span id=t></span></div><a id=link>a</a>\
    </li><li id=second><p class=rel id=rel>r</p></li></ul></main><footer><b>f</b><i>g</i></footer>";

#[test]
fn ancestor_anchor_and_its_descendant_dependents_update_without_a_document_refresh() {
    let (recomputed, total) = refresh_matches_full_cascade(
        "li:has(.x){color:red} li:has(.x) a{color:blue} footer b{color:green}",
        LIST,
        |dom| add_class(dom, "t", "x"),
    );
    assert!(recomputed < total, "{recomputed}/{total}");
}

#[test]
fn later_sibling_dependents_refresh_from_the_anchor_parent() {
    refresh_matches_full_cascade("li:has(.x) ~ li .rel{color:green}", LIST, |dom| {
        add_class(dom, "t", "x")
    });
}

#[test]
fn earlier_sibling_anchors_follow_next_and_later_sibling_relations() {
    let body = "<div><p class=a id=p>p</p><q class=a>q</q><section class=b id=s><span id=t></span>\
        </section></div><footer><b>f</b></footer>";
    for css in [
        ".a:has(+ .b .x){color:red}",
        ".a:has(~ .b .x){color:red}",
        ".a:has(+ .b .x) + .b{color:blue}",
    ] {
        refresh_matches_full_cascade(css, body, |dom| add_class(dom, "t", "x"));
    }
}

#[test]
fn negated_and_functional_anchors_use_their_element_keys() {
    for css in [
        "li:not(:has(.x)){color:red}",
        "li:is(:has(.x), .never){color:red}",
        "main li:has(.x) a{color:blue}",
    ] {
        let (recomputed, total) =
            refresh_matches_full_cascade(css, LIST, |dom| add_class(dom, "t", "x"));
        assert!(recomputed < total, "{css}: {recomputed}/{total}");
    }
}

#[test]
fn insertion_and_removal_update_ancestor_anchors() {
    let css = "li:has(.x){color:red} li:has(.x) a{color:blue}";
    refresh_matches_full_cascade(css, LIST, |dom| {
        let parent = by_id(dom, "t").parent().unwrap();
        let inserted = Node::create_element_for(&dom.document, "em");
        inserted.set_attr("class", "x");
        Node::append_child(&parent, inserted);
        parent
    });
    refresh_matches_full_cascade(
        css,
        &LIST.replace("<span id=t></span>", "<span id=t class=x></span>"),
        |dom| {
            let target = by_id(dom, "t");
            let parent = target.parent().unwrap();
            Node::remove_child(&parent, &target);
            parent
        },
    );
}

#[test]
fn keyless_anchors_walk_every_ancestor_and_filtered_has_refreshes_the_document() {
    for css in [
        ":has(.x){color:red}",
        "*:has(.x) a{color:blue}",
        "li:has(.x), :not(:has(.y)){color:red}",
        // Keys inside an argument are not hoisted to the compound that owns :has().
        ":is(main li):has(.x) a{color:blue}",
        "main :has(> span.x){color:red}",
    ] {
        refresh_matches_full_cascade(css, LIST, |dom| add_class(dom, "t", "x"));
    }
    // A keyless anchor whose result is unaffected keeps unrelated subtrees untouched.
    let (recomputed, total) = refresh_matches_full_cascade(
        "main :has(> button.p){color:red} li:has(.x) a{color:blue}",
        LIST,
        |dom| add_class(dom, "t", "x"),
    );
    assert!(recomputed < total, "{recomputed}/{total}");
    // An `of S` filter positions other elements, so it still refreshes the whole document.
    let (recomputed, total) =
        refresh_matches_full_cascade("li:nth-child(1 of :has(.x)){color:red}", LIST, |dom| {
            add_class(dom, "t", "x")
        });
    assert_eq!(recomputed, total);
}

#[test]
fn unchanged_page_level_anchor_results_keep_later_refreshes_bounded() {
    let css = "body:has(.modal) .t{color:red} li:has(.x) a{color:blue}";
    let dom = dom::parse(&format!("<style>{css}</style><body>{LIST}</body>"));
    let mut styles = StyleSet::from_sources_for_layout(&dom, "", &[], environment());
    let total = styles.styles.len();
    let refresh = |styles: &mut StyleSet, class: &str| {
        let root = add_class(&dom, "t", class);
        let stats = styles.refresh_subtrees(&dom.document, &[root], &[]);
        assert_eq!(
            styles.styles,
            StyleSet::from_sources_for_layout(&dom, "", &[], environment()).styles,
            "{class}"
        );
        stats.recomputed_styles
    };
    // Results recorded by the initial cascade keep the first refresh bounded too.
    let bounded = refresh(&mut styles, "x");
    let body = Node::composed_descendants(&dom.elements_named("body").next().unwrap()).count();
    assert!(bounded < body, "{bounded}/{body}/{total}");
    assert!(refresh(&mut styles, "") < body);
    assert!(refresh(&mut styles, "x") < body);
    // A page-level result that does change still refreshes the anchor's subtree.
    assert_eq!(refresh(&mut styles, "modal"), body);
    assert_eq!(refresh(&mut styles, "x"), body);
}

#[test]
fn flips_around_an_unbounded_refresh_never_reuse_stale_results() {
    let css = "li:has(.x) a{color:blue} li:has(.x) ~ li .rel{color:green}";
    let dom = dom::parse(&format!("<style>{css}</style><body>{LIST}</body>"));
    let mut styles = StyleSet::from_sources_for_layout(&dom, "", &[], environment());
    let assert_fresh = |styles: &StyleSet, step: &str| {
        assert_eq!(
            styles.styles,
            StyleSet::from_sources_for_layout(&dom, "", &[], environment()).styles,
            "{step}"
        );
    };
    let root = add_class(&dom, "t", "x");
    styles.refresh_subtrees(&dom.document, &[root], &[]);
    assert_fresh(&styles, "set");
    by_id(&dom, "t").set_attr("class", "");
    styles.refresh_subtrees(&dom.document, std::slice::from_ref(&dom.document), &[]);
    assert_fresh(&styles, "document refresh");
    let root = add_class(&dom, "t", "x");
    styles.refresh_subtrees(&dom.document, &[root], &[]);
    assert_fresh(&styles, "set again after document refresh");
    let root = add_class(&dom, "t", "");
    styles.refresh_subtrees(&dom.document, &[root], &[]);
    assert_fresh(&styles, "clear");
}

/// Walks from several roots share their ancestors and visit every earlier sibling of each walked
/// node, whichever root reaches a level first.
#[test]
fn walks_from_several_roots_recheck_every_earlier_sibling_once() {
    let css = "li:has(~ .x){color:red} li:has(+ .y) b{color:blue}";
    let body = "<ul><li id=a><b>a</b></li><li id=b><b>b</b></li><li id=c><b>c</b></li>\
        <li id=d><b>d</b><i id=deep>i</i></li><li id=e><b>e</b></li></ul>";
    for order in [["e", "deep", "b"], ["b", "deep", "e"]] {
        let dom = dom::parse(&format!("<style>{css}</style><body>{body}</body>"));
        let mut styles = StyleSet::from_sources_for_layout(&dom, "", &[], environment());
        by_id(&dom, "e").set_attr("class", "x");
        by_id(&dom, "c").set_attr("class", "y");
        let roots = order.map(|id| by_id(&dom, id));
        let stats = styles.refresh_subtrees(&dom.document, &roots, &[]);
        let fresh = StyleSet::from_sources_for_layout(&dom, "", &[], environment());
        assert!(stats.recomputed_styles < fresh.styles.len(), "{stats:?}");
        for node in Node::descendants(&dom.document) {
            assert_eq!(
                styles.styles.get(&node.id()),
                fresh.styles.get(&node.id()),
                "{order:?} :: <{}> {:?}",
                node.tag_name().unwrap_or_default(),
                node.attr("id")
            );
        }
    }
}
