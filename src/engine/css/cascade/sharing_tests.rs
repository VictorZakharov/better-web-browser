//! Style sharing hits and each rule that keeps an element out of the cache. Debug builds also
//! recompute every hit with a full cascade and assert equality (see `sharing`).
use super::*;

const GREEN: Color = Color::rgb(0, 128, 0);

fn styled(html: &str) -> (Dom, StyleSet, sharing::SharingStats) {
    let dom = dom::parse(html);
    let styles = StyleSet::from_dom(&dom, &[], 800.0);
    (dom, styles, sharing::LAST_STATS.with(|stats| stats.get()))
}

fn colors(dom: &Dom, styles: &StyleSet, tag: &str) -> Vec<bool> {
    dom.elements_named(tag)
        .map(|node| styles.get(&node).color == GREEN)
        .collect()
}

#[test]
fn equal_siblings_share_styles() {
    let (dom, styles, stats) = styled(
        "<style>li.a {color:green; padding:3px}</style><ul><li class=a>1</li><li class=a>2</li><li class=a>3</li></ul>",
    );
    assert_eq!(colors(&dom, &styles, "li"), [true, true, true]);
    assert!(stats.hits >= 2, "{stats:?}");
}

#[test]
fn cousins_share_through_parents_that_shared() {
    let (dom, styles, stats) = styled(
        "<style>td {color:green}</style><table><tr><td>1</td></tr><tr><td>2</td></tr><tr><td>3</td></tr></table>",
    );
    assert_eq!(colors(&dom, &styles, "td"), [true, true, true]);
    // Two rows share with the first row, then their cells share with its cell.
    assert!(stats.hits >= 4, "{stats:?}");
}

#[test]
fn structural_pseudo_classes_are_revalidated() {
    let (dom, styles, _) = styled(
        "<style>li:nth-child(2) {color:green} p:empty {color:green}</style><ul><li>1</li><li>2</li><li>3</li></ul><div><p>x</p><p></p></div>",
    );
    assert_eq!(colors(&dom, &styles, "li"), [false, true, false]);
    assert_eq!(colors(&dom, &styles, "p"), [false, true]);
}

#[test]
fn subject_position_is_compared_as_state_facts() {
    let (dom, styles, stats) = styled(
        "<style>.l > :first-child {color:green} .l > li:last-child {color:green} .t > i:first-of-type {color:green} .t > i:last-of-type {color:green} .o :only-child {color:green}</style><ul class=l><li>1</li><li>2</li><li>3</li><li>4</li><li>5</li></ul><p class=t><b></b><i></i><b></b><i></i><i></i></p><div class=o><span><em>x</em></span><span><em>y</em><em>z</em></span></div>",
    );
    assert_eq!(
        colors(&dom, &styles, "li"),
        [true, false, false, false, true]
    );
    assert_eq!(colors(&dom, &styles, "i"), [true, false, true]);
    assert_eq!(colors(&dom, &styles, "em"), [true, false, false]);
    // The middle items share with each other: their facts agree.
    assert!(stats.hits >= 2, "{stats:?}");
}

#[test]
fn ancestor_structure_is_revalidated_for_cousins() {
    let (dom, styles, _) = styled(
        "<style>tr:first-child td {color:green}</style><table><tr><td>1</td></tr><tr><td>2</td></tr></table>",
    );
    assert_eq!(colors(&dom, &styles, "td"), [true, false]);
}

#[test]
fn sibling_combinators_are_revalidated() {
    let (dom, styles, _) = styled(
        "<style>.b + .a {color:green}</style><div><span class=a></span><span class=b></span><span class=a></span><span class=a></span></div>",
    );
    assert_eq!(colors(&dom, &styles, "span"), [false, false, true, false]);
}

#[test]
fn elements_with_ids_and_inline_styles_are_not_shared() {
    let (dom, styles, stats) = styled(
        "<style>#x {color:green}</style><div><p class=a></p><p class=a id=x></p><p class=a style='color:green'></p></div>",
    );
    assert_eq!(colors(&dom, &styles, "p"), [false, true, true]);
    assert_eq!(stats.hits, 0, "{stats:?}");
}

#[test]
fn attribute_selectors_compare_selector_only_attribute_values() {
    let (dom, styles, _) = styled(
        "<style>[data-k=v] {color:green} a[href$='.pdf'] {color:green}</style><div><i data-k=v></i><i data-k=w></i><a href=a.html></a><a href=b.pdf></a></div>",
    );
    assert_eq!(colors(&dom, &styles, "i"), [true, false]);
    assert_eq!(colors(&dom, &styles, "a"), [false, true]);
}

#[test]
fn presentational_attribute_values_are_compared() {
    let (dom, styles, _) = styled(
        "<table><tr><td align=left bgcolor=red></td><td align=left bgcolor=green></td></tr></table>",
    );
    let cells = dom.elements_named("td").collect::<Vec<_>>();
    assert_eq!(styles.get(&cells[1]).background_color, GREEN);
    assert_ne!(styles.get(&cells[0]).background_color, GREEN);
}

#[test]
fn element_state_is_revalidated() {
    let dom = dom::parse(
        "<style>li:hover {color:green} ul:hover li {padding:2px}</style><ul><li>1</li><li>2</li></ul>",
    );
    let second = dom.elements_named("li").nth(1).unwrap();
    second.element().unwrap().hovered.set(true);
    let styles = StyleSet::from_dom(&dom, &[], 800.0);
    assert_eq!(colors(&dom, &styles, "li"), [false, true]);
}

#[test]
fn language_and_scoped_rules_are_respected() {
    let (dom, styles, _) = styled(
        "<style>:lang(fr) p {color:green} @scope (.s) { p {padding:4px} }</style><div lang=fr><p>a</p></div><div lang=en><p>b</p></div><div class=s><p>c</p><p>d</p></div>",
    );
    assert_eq!(colors(&dom, &styles, "p"), [true, false, false, false]);
    let padded = dom
        .elements_named("p")
        .map(|node| styles.get(&node).padding.top != Length::Px(0.0))
        .collect::<Vec<_>>();
    assert_eq!(padded, [false, false, true, true]);
}

#[test]
fn form_controls_and_custom_elements_are_not_shared() {
    let (_, _, stats) =
        styled("<div><input><input><x-a></x-a><x-a></x-a><button></button><button></button></div>");
    assert_eq!(stats.hits, 0, "{stats:?}");
}

#[test]
fn incremental_restyles_share_between_restyled_siblings() {
    let (dom, mut styles, _) = styled(
        "<style>.on li {color:green; padding:2px}</style><ul><li>1</li><li>2</li><li>3</li></ul>",
    );
    assert_eq!(colors(&dom, &styles, "li"), [false, false, false]);
    let list = dom.elements_named("ul").next().unwrap();
    list.set_attr("class", "on");
    let stats = styles.refresh_subtrees(&dom.document, std::slice::from_ref(&list), &[]);
    assert_eq!(colors(&dom, &styles, "li"), [true, true, true]);
    assert!(stats.changed_styles >= 4, "{stats:?}");
    let sharing = sharing::LAST_STATS.with(|stats| stats.get());
    assert!(sharing.hits >= 2, "{sharing:?}");
}

#[test]
fn compared_attributes_and_simple_negations_need_no_revalidation() {
    // `:not(.skip)` and `[title]` follow from the compared class list and attribute names,
    // and `[lang|=en]` from a compared value; `[data-x=1]` still revalidates per element.
    let (dom, styles, stats) = styled(
        "<style>li:not(.skip)[title] {color:green} a[lang|=en] {color:green} li[data-x='1'][title] {color:red}</style><ul><li title=a>1</li><li title=b>2</li><li class=skip title=c>3</li><li data-x=1 title=d>4</li><li data-x=2 title=e>5</li></ul><p><a lang=en>x</a><a lang=fr>y</a></p>",
    );
    assert_eq!(
        colors(&dom, &styles, "li"),
        [true, true, false, false, true]
    );
    assert_eq!(colors(&dom, &styles, "a"), [true, false]);
    assert!(stats.hits >= 1, "{stats:?}");
}

#[test]
fn shared_styles_and_text_under_them_reference_one_allocation() {
    let (dom, styles, _) = styled(
        "<style>p.a {color:green}</style><div><p class=a>one</p><p class=a>two</p></div><details><summary>s</summary>hidden</details>",
    );
    let paragraphs = dom.elements_named("p").collect::<Vec<_>>();
    let style = |node: &NodeRef| &styles.styles[&node.id()];
    assert!(Arc::ptr_eq(style(&paragraphs[0]), style(&paragraphs[1])));
    let text = |node: &NodeRef| node.children.borrow()[0].clone();
    // Text inherits from its parent alone, so text under one shared style shares it too.
    assert!(Arc::ptr_eq(
        style(&text(&paragraphs[0])),
        style(&text(&paragraphs[1]))
    ));
    assert_eq!(style(&text(&paragraphs[1])).color, GREEN);
    // A closed `details` element hides its non-summary text; that text is styled on its own.
    let details = dom.elements_named("details").next().unwrap();
    let hidden = details.children.borrow().last().cloned().unwrap();
    assert_eq!(style(&hidden).display, Display::None);
}

#[test]
fn attribute_order_and_annotation_attributes_do_not_prevent_sharing() {
    // Order never affects style, and RDFa/microdata annotations are read only by selectors,
    // which still tell the elements apart when a rule names them.
    let (dom, styles, stats) = styled(
        "<style>span.r {color:green} span.r[about='#b'] {color:red}</style><p><span class=r id=x1 about='#a' typeof=T>1</span><span id=x2 class=r about='#c' typeof=U>2</span><span class=r about='#b' id=x3 typeof=T>3</span></p>",
    );
    assert_eq!(colors(&dom, &styles, "span"), [true, true, false]);
    assert!(stats.hits >= 1, "{stats:?}");
}

#[test]
fn sharing_elements_reuse_generated_styles_only_when_the_same_rules_apply() {
    let (dom, styles, stats) = styled(
        "<style>li::before {content:'*'; color:green} li:nth-child(2)::before {color:red}          li[data-n]::after {content:attr(data-n)}</style>         <ul><li>a</li><li>b</li><li>c</li><li data-n=1>d</li><li data-n=2>e</li></ul>",
    );
    let items = dom.elements_named("li").collect::<Vec<_>>();
    let before = |item: &NodeRef| styles.pseudo_styles[&(item.id(), PseudoElement::Before)].color;
    // The `:nth-child(2)` rule is revalidated: the second item computes its own style, and
    // the third reuses the first item's.
    let colors = items.iter().map(before).collect::<Vec<_>>();
    let red = Color::rgb(255, 0, 0);
    assert_eq!(colors, [GREEN, red, GREEN, GREEN, GREEN]);
    assert!(Arc::ptr_eq(
        &styles.pseudo_styles[&(items[2].id(), PseudoElement::Before)],
        &styles.pseudo_styles[&(items[0].id(), PseudoElement::Before)],
    ));
    // A shared `::after` style still resolves `attr()` content for its own element.
    let after = |item: &NodeRef| {
        styles
            .generated_nodes
            .get(&(item.id(), PseudoElement::After))
            .map(|node| node.text_content())
    };
    assert_eq!(after(&items[2]), None);
    assert_eq!(after(&items[3]).as_deref(), Some("1"));
    assert_eq!(after(&items[4]).as_deref(), Some("2"));
    assert!(stats.pseudo_hits >= 4, "{stats:?}");
}
