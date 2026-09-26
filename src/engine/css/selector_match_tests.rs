use super::*;

#[test]
fn matches_adjacent_and_general_element_siblings() {
    let dom = dom::parse("<i></i>text<b id='one'></b><b id='two'></b>");
    let two = dom
        .find_node(dom.elements_named("b").nth(1).unwrap().id())
        .unwrap();
    assert!(selector_matches(&parse_selector("b + b").unwrap(), &two));
    assert!(selector_matches(&parse_selector("i ~ b").unwrap(), &two));
    assert!(!selector_matches(&parse_selector("i + b").unwrap(), &two));
}

#[test]
fn matches_enabled_and_disabled_form_controls() {
    let dom = dom::parse("<button id=on></button><button id=off disabled></button>");
    let buttons = dom.elements_named("button").collect::<Vec<_>>();
    assert!(selector_matches(
        &parse_selector("button:enabled").unwrap(),
        &buttons[0]
    ));
    assert!(selector_matches(
        &parse_selector("button:disabled").unwrap(),
        &buttons[1]
    ));
}

#[test]
fn matches_first_element_of_the_same_expanded_type() {
    let dom = dom::parse("<i></i>text<b id='first'></b><em></em><b id='second'></b>");
    let nodes = dom.elements_named("b").collect::<Vec<_>>();
    let selector = parse_selector("b:first-of-type").unwrap();

    assert!(selector_matches(&selector, &nodes[0]));
    assert!(!selector_matches(&selector, &nodes[1]));
}

#[test]
fn functional_pseudo_classes_match_attribute_selectors() {
    let dom =
        dom::parse("<main is-two-columns_ force-default-style></main><main id='single'></main>");
    let nodes = dom.elements_named("main").collect::<Vec<_>>();

    assert!(!selector_matches(
        &parse_selector("main:not([is-two-columns_])").unwrap(),
        &nodes[0]
    ));
    assert!(selector_matches(
        &parse_selector("main:not([is-two-columns_])").unwrap(),
        &nodes[1]
    ));
    assert!(selector_matches(
        &parse_selector("main:is([force-default-style], .fallback)").unwrap(),
        &nodes[0]
    ));
}

#[test]
fn functional_pseudo_classes_match_complex_selector_lists() {
    let dom = dom::parse(
        "<section id='active'><article><p class='item'></p></article></section><aside><p class='item'></p></aside>",
    );
    let nodes = dom.elements_named("p").collect::<Vec<_>>();
    let is_selector = parse_selector("p:is(section > article p, aside > p)").unwrap();
    assert!(selector_matches(&is_selector, &nodes[0]));
    assert!(selector_matches(&is_selector, &nodes[1]));

    let not_selector = parse_selector("p:not(#active article p, .missing)").unwrap();
    assert!(!selector_matches(&not_selector, &nodes[0]));
    assert!(selector_matches(&not_selector, &nodes[1]));

    let where_selector = parse_selector("p:where(section article p)").unwrap();
    assert!(selector_matches(&where_selector, &nodes[0]));
    assert!(!selector_matches(&where_selector, &nodes[1]));
    assert_eq!(where_selector.specificity.classes, 0);
    assert_eq!(where_selector.specificity.tags, 1);
}

#[test]
fn functional_selector_specificity_uses_the_most_specific_member() {
    let is_selector = parse_selector("p:is(.item, #active article p)").unwrap();
    assert_eq!(is_selector.specificity.ids, 1);
    assert_eq!(is_selector.specificity.tags, 3);

    let not_selector = parse_selector("p:not(.item, #active article p)").unwrap();
    assert_eq!(not_selector.specificity.ids, 1);
    assert_eq!(not_selector.specificity.tags, 3);

    let where_selector = parse_selector("p:where(.item, #active article p)").unwrap();
    assert_eq!(where_selector.specificity.ids, 0);
    assert_eq!(where_selector.specificity.tags, 1);
}

#[test]
fn relational_selector_matches_descendants_children_and_following_siblings() {
    let dom = dom::parse(
        "<main><section id=first><div class=inner><b class=mark></b></div></section>
         <section id=second><b class=mark></b></section>
         <section id=third></section><aside><span class=target></span></aside></main>",
    );
    let sections = dom.elements_named("section").collect::<Vec<_>>();
    let matches =
        |source: &str, node: &NodeRef| selector_matches(&parse_selector(source).unwrap(), node);
    assert!(matches("section:has(.mark)", &sections[0]));
    assert!(!matches("section:has(> .mark)", &sections[0]));
    assert!(matches("section:has(> .mark)", &sections[1]));
    assert!(matches("section:has(+ section > .mark)", &sections[0]));
    assert!(!matches("section:has(+ section > .mark)", &sections[1]));
    assert!(matches("section:has(+ aside .target)", &sections[2]));
    assert!(matches("section:has(~ aside .target)", &sections[0]));
    assert!(!matches("section:has(~ .missing)", &sections[0]));
}

#[test]
fn relational_selector_uses_max_argument_specificity_and_rejects_nesting() {
    let selector = parse_selector("section:has(.mark, #id > b)").unwrap();
    assert_eq!(selector.specificity.ids, 1);
    assert_eq!(selector.specificity.tags, 2);
    assert!(parse_selector("section:has(.mark, :unsupported)").is_none());
    assert!(parse_selector("section:has(:has(.mark))").is_none());
}

#[test]
fn structural_pseudos_count_element_siblings_and_filtered_nth_positions() {
    let dom = dom::parse(
        "<ul><li class=hit id=one></li>text<span></span><li id=two></li>
         <li class=hit id=three></li><li class=hit id=four></li></ul>",
    );
    let items = dom.elements_named("li").collect::<Vec<_>>();
    let matches =
        |source: &str, node: &NodeRef| selector_matches(&parse_selector(source).unwrap(), node);
    assert!(matches("li:first-child", &items[0]));
    assert!(matches("li:last-child", &items[3]));
    assert!(!matches("li:nth-child(2)", &items[1]));
    assert!(matches("li:nth-child(3)", &items[1]));
    assert!(matches("li:nth-of-type(2)", &items[1]));
    assert!(matches("li:nth-last-of-type(2)", &items[2]));
    assert!(matches("li:nth-child(odd)", &items[1]));
    assert!(matches("li:nth-child(2 of .hit)", &items[2]));
    assert!(!matches("li:nth-child(2 of .hit)", &items[1]));
    assert!(matches("li:nth-last-child(1 of .hit)", &items[3]));
    assert!(matches("li:nth-child(-n+2 of .hit)", &items[0]));
    assert!(matches("li:nth-child(-n+2 of .hit)", &items[2]));
    assert!(!matches("li:nth-child(-n+2 of .hit)", &items[3]));
}

#[test]
fn empty_and_only_child_ignore_comments_and_whitespace_not_elements() {
    let dom = dom::parse(
        "<main><p id=blank> \n<!-- comment -->\t</p><p id=text>content</p>
         <section><b id=only></b></section><section><b></b><i></i></section></main>",
    );
    let blank = dom.elements_named("p").next().unwrap();
    let text = dom.elements_named("p").nth(1).unwrap();
    let only = dom.elements_named("b").next().unwrap();
    let other = dom.elements_named("b").nth(1).unwrap();
    let matches =
        |source: &str, node: &NodeRef| selector_matches(&parse_selector(source).unwrap(), node);
    assert!(matches("p:empty", &blank));
    assert!(!matches("p:empty", &text));
    assert!(matches("b:only-child", &only));
    assert!(matches("b:only-of-type", &only));
    assert!(!matches("b:only-child", &other));
    assert!(matches("b:only-of-type", &other));
}

#[test]
fn escaped_ids_classes_and_quoted_combinators_match_dom_values() {
    let dom = dom::parse(
        "<main><div id='123' class='a+b' data-label='a > b'></div><span id=next></span></main>",
    );
    let div = dom.elements_named("div").next().unwrap();
    let span = dom.elements_named("span").next().unwrap();
    let exact = parse_selector(r#"#\31 23.a\+b[data-label="a > b"]"#).unwrap();
    assert!(selector_matches(&exact, &div));
    let adjacent = parse_selector(r#"[data-label="a > b"] + span"#).unwrap();
    assert!(selector_matches(&adjacent, &span));
}

#[test]
fn quoted_commas_remain_inside_functional_selector_members() {
    let dom = dom::parse("<div data-label='a,b'></div><div data-label='a'></div>");
    let nodes = dom.elements_named("div").collect::<Vec<_>>();
    let selector = parse_selector("div:is([data-label='a,b'], .other)").unwrap();
    assert!(selector_matches(&selector, &nodes[0]));
    assert!(!selector_matches(&selector, &nodes[1]));
}

#[test]
fn html_attribute_defaults_are_case_insensitive_only_for_enumerated_names() {
    let dom = dom::parse("<input type=TEXT data-mode=TEXT rel=NoFoLlOw>");
    let input = dom.elements_named("input").next().unwrap();
    let matches = |source: &str| selector_matches(&parse_selector(source).unwrap(), &input);
    assert!(matches("[type=text]"));
    assert!(matches("[rel=nofollow]"));
    assert!(!matches("[type=text s]"));
    assert!(!matches("[data-mode=text]"));
    assert!(matches("[data-mode=text i]"));
    assert!(matches("[type^=te]"));
    assert!(!matches("[type^=te s]"));
}

#[test]
fn empty_attribute_operands_do_not_match_substrings_or_tokens() {
    let dom = dom::parse("<div data-empty='' data-value=hello></div>");
    let node = dom.elements_named("div").next().unwrap();
    let matches = |source: &str| selector_matches(&parse_selector(source).unwrap(), &node);
    assert!(matches("[data-empty='']"));
    assert!(!matches("[data-value='']"));
    for source in [
        "[data-empty~='']",
        "[data-empty|='']",
        "[data-empty^='']",
        "[data-empty$='']",
        "[data-empty*='']",
        "[data-value^='']",
        "[data-value$='']",
        "[data-value*='']",
    ] {
        assert!(!matches(source), "{source}");
    }
}

#[test]
fn language_selectors_inherit_and_filter_extended_ranges() {
    let dom = dom::parse(
        "<main lang=de-Latn-DE><p id=inherit></p><p id=override lang=fr-CH></p>
         <p id=empty lang=''></p></main>",
    );
    let nodes = dom.elements_named("p").collect::<Vec<_>>();
    let matches =
        |source: &str, node: &NodeRef| selector_matches(&parse_selector(source).unwrap(), node);
    assert!(matches("p:lang(de-DE)", &nodes[0]));
    assert!(matches("p:lang(de, fr)", &nodes[0]));
    assert!(matches(r#"p:lang("*-CH")"#, &nodes[1]));
    assert!(!matches("p:lang(de)", &nodes[1]));
    assert!(matches("p:lang(\"\")", &nodes[2]));
    assert!(!matches("p:lang(\"*\")", &nodes[2]));
    assert!(!matches("p:lang(de):lang(fr)", &nodes[0]));
}

#[test]
fn focus_and_focus_within_follow_the_focused_element() {
    let dom = dom::parse(
        "<main><section id=first><input id=a></section><section id=second><input id=b></section></main>",
    );
    let inputs = dom.elements_named("input").collect::<Vec<_>>();
    let sections = dom.elements_named("section").collect::<Vec<_>>();
    let focus = parse_selector("input:focus").unwrap();
    let within = parse_selector("section:focus-within").unwrap();
    Node::set_focus_target(None, Some(&inputs[0]));
    assert!(selector_matches(&focus, &inputs[0]));
    assert!(selector_matches(&within, &sections[0]));
    assert!(!selector_matches(&within, &sections[1]));
    Node::set_focus_target(Some(&inputs[0]), Some(&inputs[1]));
    assert!(!selector_matches(&focus, &inputs[0]));
    assert!(!selector_matches(&within, &sections[0]));
    assert!(selector_matches(&within, &sections[1]));
    Node::set_focus_target(Some(&inputs[1]), None);
    assert!(!selector_matches(&focus, &inputs[1]));
    assert!(!selector_matches(&within, &sections[1]));
}

#[test]
fn focus_within_follows_reparented_focused_subtree() {
    let dom = dom::parse(
        "<main><section id=old><div><input></div></section><section id=new></section></main>",
    );
    let sections = dom.elements_named("section").collect::<Vec<_>>();
    let subtree = dom.elements_named("div").next().unwrap();
    let field = dom.elements_named("input").next().unwrap();
    Node::set_focus_target(None, Some(&field));
    assert!(sections[0].has_focus_within());
    assert!(Node::append_child(&sections[1], subtree));
    assert!(!sections[0].has_focus_within());
    assert!(sections[1].has_focus_within());
    Node::set_focus_target(Some(&field), None);
    assert!(!sections[1].has_focus_within());
}

#[test]
fn directionality_matches_inherited_and_auto_first_strong_text() {
    let dom = dom::parse(
        "<main dir=rtl><p id=inherited>English</p><p dir=ltr>עברית</p>
         <div dir=auto><span>123 עברית</span></div>
         <div dir=auto><b dir=rtl>עברית</b>English</div>
         <input dir=auto value='עברית'>
         <input type=tel dir=rtl value='עברית'></main>",
    );
    let paragraphs = dom.elements_named("p").collect::<Vec<_>>();
    let divs = dom.elements_named("div").collect::<Vec<_>>();
    let inputs = dom.elements_named("input").collect::<Vec<_>>();
    let matches =
        |source: &str, node: &NodeRef| selector_matches(&parse_selector(source).unwrap(), node);
    assert!(matches(":dir(rtl)", &paragraphs[0]));
    assert!(matches(":dir(ltr)", &paragraphs[1]));
    assert!(matches(":dir(rtl)", &divs[0]));
    assert!(matches(":dir(ltr)", &divs[1]));
    assert!(matches(":dir(rtl)", &inputs[0]));
    assert!(matches(":dir(rtl)", &inputs[1]));
    assert!(!matches(":dir(ltr):dir(rtl)", &paragraphs[0]));
}
