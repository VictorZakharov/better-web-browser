//! Selector matching regressions independent of computed value parsing.
use super::*;

#[test]
fn matches_descendants_children_compounds_and_not() {
    let dom = dom::parse(
        r#"<style>
                #app > .row a.link { color: rgb(1,2,3); }
                .row:not(.hidden) { background-color: #abcdef; }
               </style><div id="app"><div class="row"><a class="link">x</a></div></div>"#,
    );
    let styles = StyleSet::from_dom(&dom, &[], 1000.0);
    let link = dom.elements_named("a").next().unwrap();
    let row = dom
        .elements_named("div")
        .find(|node| node.has_class("row"))
        .unwrap();
    assert_eq!(styles.get(&link).color, Color::rgb(1, 2, 3));
    assert_eq!(
        styles.get(&row).background_color,
        Color::rgb(0xab, 0xcd, 0xef)
    );
}

#[test]
fn matches_functional_selector_lists_root_and_has() {
    let dom = dom::parse(
        r#"<style>
                :root { background-color: #010203; }
                :is(#links, #ads) .result { color: #123456; }
                p:not(.muted, .hidden) { background-color: #abcdef; }
                .outside:has(.result) { color: red; }
               </style>
               <main id="links"><p class="result">shown</p></main>
               <p class="muted">muted</p><div class="outside"><span class="result">x</span></div>"#,
    );
    let styles = StyleSet::from_dom(&dom, &[], 1000.0);
    let html = dom.elements_named("html").next().unwrap();
    let result = dom
        .elements_named("p")
        .find(|node| node.has_class("result"))
        .unwrap();
    let muted = dom
        .elements_named("p")
        .find(|node| node.has_class("muted"))
        .unwrap();
    let outside = dom
        .elements_named("div")
        .find(|node| node.has_class("outside"))
        .unwrap();
    assert_eq!(styles.get(&html).background_color, Color::rgb(1, 2, 3));
    assert_eq!(
        styles.get(&result).background_color,
        Color::rgb(0xab, 0xcd, 0xef)
    );
    assert_eq!(styles.get(&result).color, Color::rgb(0x12, 0x34, 0x56));
    assert_eq!(styles.get(&muted).background_color, Color::TRANSPARENT);
    assert_eq!(styles.get(&outside).color, Color::rgb(255, 0, 0));
}

#[test]
fn matches_attribute_selectors_instead_of_treating_them_as_wildcards() {
    let dom = dom::parse(
        r#"<style>
                .item[data-display="block"] { display: block; color: green; }
                .item[data-display="none"] { display: none; color: red; }
                [data-tags~="featured"] { background-color: #123456; }
               </style>
               <div class="item" data-display="block" data-tags="home featured">visible</div>
               <div class="item" data-display="none">hidden</div>"#,
    );
    let styles = StyleSet::from_dom(&dom, &[], 1000.0);
    let mut items = dom
        .elements_named("div")
        .filter(|node| node.has_class("item"));
    let visible = items.next().unwrap();
    let hidden = items.next().unwrap();
    assert_eq!(styles.get(&visible).display, Display::Block);
    assert_eq!(styles.get(&visible).color, Color::rgb(0, 128, 0));
    assert_eq!(
        styles.get(&visible).background_color,
        Color::rgb(0x12, 0x34, 0x56)
    );
    assert_eq!(styles.get(&hidden).display, Display::None);
    assert_eq!(styles.get(&hidden).color, Color::rgb(255, 0, 0));
}
