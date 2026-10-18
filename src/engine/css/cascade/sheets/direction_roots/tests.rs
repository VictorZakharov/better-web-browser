//! Bounded `:dir()` refreshes must equal a whole-document cascade.
use crate::engine::css::cascade::StyleSet;
use crate::engine::css::media::MediaEnvironment;
use crate::engine::dom::{self, Dom, Node, NodeRef};

fn by_id(dom: &Dom, id: &str) -> NodeRef {
    Node::descendants(&dom.document)
        .find(|node| node.attr("id").as_deref() == Some(id))
        .unwrap_or_else(|| panic!("#{id}"))
}

/// Applies `mutate`, refreshes from the root it returns (as the ordinary invalidation records
/// would), and compares every computed style with a fresh cascade. Returns (recomputed, total).
fn refresh_matches_full_cascade(
    css: &str,
    body: &str,
    mutate: impl FnOnce(&Dom) -> NodeRef,
) -> (usize, usize) {
    let environment = MediaEnvironment::new(800.0, 600.0, 1.0, false);
    let dom = dom::parse(&format!("<style>{css}</style><body>{body}</body>"));
    let mut styles = StyleSet::from_sources_for_layout(&dom, "", &[], environment);
    let root = mutate(&dom);
    let stats = styles.refresh_subtrees(&dom.document, &[root], &[]);
    let fresh = StyleSet::from_sources_for_layout(&dom, "", &[], environment);
    for node in Node::descendants(&dom.document) {
        assert_eq!(
            styles.styles.get(&node.id()),
            fresh.styles.get(&node.id()),
            "{css} :: <{}> {:?} in {body}",
            node.tag_name().unwrap_or_default(),
            node.attr("id")
        );
    }
    (stats.recomputed_styles, fresh.styles.len())
}

/// Appends right-to-left text to `#id`; a child-list change records the parent.
fn append_rtl_text(dom: &Dom, id: &str) -> NodeRef {
    let parent = by_id(dom, id);
    Node::append_child(&parent, Node::create_text("\u{5d0}"));
    parent
}

fn filler() -> String {
    "<section><p>a <b>b</b></p><p>c</p></section>".repeat(20)
}

#[test]
fn text_outside_auto_directionality_restyles_only_its_subtree() {
    let body = format!(
        "<main>{}<p id=plain></p></main><ol dir=auto><li><span class=t>x</span></li></ol>",
        filler()
    );
    for css in [".t:dir(rtl){color:red}", "ol:has(.t:dir(rtl)){color:red}"] {
        let (recomputed, total) =
            refresh_matches_full_cascade(css, &body, |dom| append_rtl_text(dom, "plain"));
        assert!(recomputed < total / 4, "{css}: {recomputed}/{total}");
    }
}

#[test]
fn auto_directionality_ancestors_restyle_their_inheriting_subtrees() {
    for (css, body) in [
        (
            ".t:dir(rtl){color:red}",
            "<div dir=auto><p id=target></p><p><span class=t>x</span></p></div>",
        ),
        (
            ".t:dir(rtl){color:red}",
            "<bdi><p id=target></p><span class=t>x</span></bdi>",
        ),
        (
            ".t:dir(rtl){color:red}",
            "<div dir=AUTO><div dir=auto><p id=target></p></div><span class=t>x</span></div>",
        ),
        (
            ".t:dir(ltr){color:green}",
            "<div dir=auto><p id=target></p><b>latin</b><span class=t>x</span></div>",
        ),
    ] {
        refresh_matches_full_cascade(css, body, |dom| append_rtl_text(dom, "target"));
    }
}

#[test]
fn siblings_and_relational_anchors_of_a_flipped_ancestor_are_refreshed() {
    let body = "<main><ol class=refs><li><span class=ref dir=auto id=target></span></li></ol>\
        <p class=after>p</p></main><footer>f</footer>";
    for css in [
        ".refs:has(.ref:dir(rtl)){direction:rtl;text-align:right}",
        ".ref:dir(rtl) ~ b, .refs:has(.ref:dir(rtl)) + .after{color:red}",
        "main:has(:dir(rtl)) footer{color:blue}",
    ] {
        refresh_matches_full_cascade(css, body, |dom| append_rtl_text(dom, "target"));
    }
}

#[test]
fn explicit_direction_ancestors_do_not_widen_the_refresh() {
    let body = format!(
        "<main>{}</main><div dir=rtl><p id=target></p><span class=t>x</span></div>",
        filler()
    );
    let (recomputed, total) =
        refresh_matches_full_cascade(".t:dir(rtl){color:red}", &body, |dom| {
            append_rtl_text(dom, "target")
        });
    assert!(recomputed < total / 4, "{recomputed}/{total}");
}
