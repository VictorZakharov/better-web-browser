use crate::engine::css::StyleRefreshStats;
use crate::engine::dom::{Node, NodeRef};
use crate::engine::invalidation::{MutationKind, RenderInvalidation};
use crate::engine::page::Page;

fn by_id(page: &Page, id: &str) -> NodeRef {
    Node::descendants(&page.dom.document)
        .find(|node| node.attr("id").as_deref() == Some(id))
        .unwrap_or_else(|| panic!("#{id}"))
}

fn by_class(root: &NodeRef, class: &str) -> NodeRef {
    Node::descendants(root)
        .find(|node| node.attr("class").as_deref() == Some(class))
        .unwrap_or_else(|| panic!(".{class}"))
}

fn element(page: &Page, tag: &str, class: &str) -> NodeRef {
    let node = Node::create_element_for(&page.dom.document, tag);
    node.set_attr("class", class);
    Node::append_child(&node, Node::create_element_for(&page.dom.document, "b"));
    node
}

enum Change {
    Append,
    Prepend,
    RemoveFirst,
    MoveLastFirst,
    ReinsertLastFirst,
    AdoptFromSection,
}

/// Applies `change` to `#parent`'s children, refreshes only the child-list roots, and asserts
/// every element matches a from-scratch cascade of the final document.
fn change_children(html: &str, change: &Change) -> StyleRefreshStats {
    let mut page = Page::parse(html, "https://example.test/");
    page.refresh_resources(800.0);
    let parent = by_id(&page, "parent");
    let mut roots = vec![parent.clone()];
    let mut removed_nodes = Vec::new();
    match change {
        Change::Append => {
            Node::append_child(&parent, element(&page, "div", "new"));
        }
        Change::Prepend => {
            let first = parent.children.borrow()[0].clone();
            Node::insert_before(&parent, element(&page, "div", "new"), &first);
        }
        Change::RemoveFirst => {
            let first = parent
                .children
                .borrow()
                .iter()
                .find(|child| child.element().is_some())
                .cloned()
                .expect("child");
            removed_nodes.extend(Node::descendants(&first).map(|node| node.id()));
            Node::remove_child(&parent, &first);
        }
        Change::MoveLastFirst => {
            let first = parent.children.borrow()[0].clone();
            let last = parent.children.borrow().last().cloned().expect("child");
            Node::insert_before(&parent, last.clone(), &first);
            // Script hosts restyle a moved subtree whole (`invalidate_previous_parent`).
            roots.push(last);
        }
        Change::ReinsertLastFirst => {
            // Removed first, so the removal log names it and no root covers it.
            let last = parent.children.borrow().last().cloned().expect("child");
            removed_nodes.extend(Node::descendants(&last).map(|node| node.id()));
            Node::remove_child(&parent, &last);
            let first = parent.children.borrow()[0].clone();
            Node::insert_before(&parent, last, &first);
        }
        Change::AdoptFromSection => {
            // A paragraph leaves a section (whose styles it inherited) for the parent.
            let section = by_class(&parent, "s");
            let paragraph = section.children.borrow()[0].clone();
            removed_nodes.extend(Node::descendants(&paragraph).map(|node| node.id()));
            Node::remove_child(&section, &paragraph);
            roots.push(section);
            Node::append_child(&parent, paragraph);
        }
    }
    let invalidation = RenderInvalidation {
        impact: MutationKind::ChildList.impact(),
        mutation_count: 1,
        roots: roots.iter().map(|root| root.id()).collect(),
        child_list_roots: vec![parent.id()],
        removed_nodes,
        ..RenderInvalidation::default()
    };
    let stats = page.refresh_resources_after_invalidation(800.0, &invalidation);
    let fresh = page.style(800.0);
    let incremental = page.cached_style(800.0).expect("refreshed styles");
    for node in Node::descendants(&page.dom.document).filter(|node| node.element().is_some()) {
        assert!(
            incremental.styles.get(&node.id()) == fresh.styles.get(&node.id()),
            "stale style for <{}{}> after {} in {html}",
            node.tag_name().unwrap_or_default(),
            node.attr("class")
                .map(|class| format!(".{class}"))
                .unwrap_or_default(),
            match change {
                Change::Append => "append",
                Change::Prepend => "prepend",
                Change::RemoveFirst => "remove",
                Change::MoveLastFirst => "move",
                Change::ReinsertLastFirst => "reinsert",
                Change::AdoptFromSection => "adopt",
            },
        );
    }
    stats
}

const CHANGES: [Change; 6] = [
    Change::Append,
    Change::Prepend,
    Change::RemoveFirst,
    Change::MoveLastFirst,
    Change::ReinsertLastFirst,
    Change::AdoptFromSection,
];

fn page(css: &str) -> String {
    let sections =
        "<section class=s><p class=x>text <b>bold</b></p><p>more</p></section>".repeat(20);
    format!(
        "<html><style>{css}</style><body id=parent><div class=a><i class=x>a</i></div>\
         {sections}<div class=b><i class=x>b</i></div></body></html>"
    )
}

#[test]
fn positional_rules_see_inserted_and_removed_siblings() {
    for css in [
        "body > :first-child{color:red} body > :last-child{color:blue}",
        "div:first-of-type{color:red} div:last-of-type{color:blue} :only-child{color:green}",
        "body > :nth-child(2n+1){color:red} :nth-last-child(2){color:blue}",
        ":nth-child(1 of .s){color:red} :nth-last-child(1 of div){color:blue}",
        ".a + section{color:red} .new ~ .b{color:blue} .new + *{color:green}",
        ":is(.a + section, :first-child) p{color:red}",
        ":first-child .x{color:red} :last-child > .x{color:blue}",
        ".a ~ div .x{color:red} section + section p{color:blue}",
        ":not(:first-child) > b{color:red}",
        "body:empty{color:red} div:empty{color:blue} :not(:empty){outline:1px solid}",
        "body:has(> .new){color:red} .a:has(+ .new){color:blue} .s:has(~ .b){color:green}",
        "body > :last-child{font-size:30px} b{color:red}",
        ".new{color:red} .new b{color:blue}",
        ".s{color:red;font-size:20px} b{border:1px solid}",
        ".s:last-child > :last-child{color:red} .s:nth-child(2) *{color:blue}",
        ".a ~ section *{color:red} :nth-child(1 of .s) > *{color:blue}",
        "body :first-child .x{color:red} html :last-child *{color:blue}",
        "html > body > :first-child b{color:red} body.y :first-child *{color:blue}",
    ] {
        for change in &CHANGES {
            change_children(&page(css), change);
        }
    }
}

#[test]
fn children_changes_do_not_restyle_the_whole_parent_subtree() {
    for css in [
        ".new{color:red}",
        "body > :first-child{color:red} body > :last-child{color:blue}",
        ".a + section{color:red} .new ~ .b{color:blue}",
        ":first-child .x{color:red}",
        // Its outer compound matches neither the parent nor an ancestor.
        ".elsewhere :first-child *{color:red} .elsewhere > * + * b{color:blue}",
    ] {
        for change in [Change::Append, Change::Prepend] {
            let stats = change_children(&page(css), &change);
            assert!(!stats.full_rebuild, "{css}");
            assert!(
                stats.recomputed_styles < stats.total_styles / 3,
                "{css}: {stats:?}"
            );
        }
    }
}

#[test]
fn universal_subjects_after_ancestor_positions_keep_the_subtree() {
    // `:first-child *` can change for any element below the parent; correctness only.
    for change in &CHANGES {
        change_children(&page(":first-child *{color:red}"), change);
    }
}

#[test]
fn universal_subjects_below_keyed_positions_restyle_matching_children() {
    for css in [
        // Keys on a few of the parent's children; `.s` would reach every section.
        ".b:last-child > :last-child{color:red}",
        ".a ~ .b *{color:red} body > div:first-child b{color:blue}",
        ".a:nth-child(1) *{color:red} .new + .b *{color:blue}",
    ] {
        for change in &CHANGES {
            change_children(&page(css), change);
        }
        let stats = change_children(&page(css), &Change::Append);
        assert!(
            stats.recomputed_styles < stats.total_styles / 3,
            "{css}: {stats:?}"
        );
    }
}

#[test]
fn empty_parents_reach_their_siblings() {
    for css in [
        ".e:empty + p{color:red}",
        ":is(.e:empty) ~ p{color:red}",
        "p:not(:empty){color:red} .e:not(:empty) + p b{color:blue}",
        ".e:not(:empty) i{color:red} div:empty *{color:blue} .e:empty > b{color:green}",
    ] {
        let html = format!(
            "<style>{css}</style><div><div id=parent class=e><i></i></div><p>x <b>y</b></p></div>"
        );
        change_children(&html, &Change::RemoveFirst);
        change_children(&html, &Change::Append);
    }
}
