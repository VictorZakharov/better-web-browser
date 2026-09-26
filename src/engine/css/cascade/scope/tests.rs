use super::*;

fn color(dom: &Dom, styles: &StyleSet, id: &str) -> Color {
    let node = Node::descendants(&dom.document)
        .find(|node| node.attr_ref("id").as_deref() == Some(id))
        .expect("test target");
    styles.get(&node).color
}

#[test]
fn scope_root_and_limit_constrain_only_the_rule_subject() {
    let dom = dom::parse(
        r#"<style>
           p { color: black }
           @scope (.card) to (> .stop) {
             p { color: red }
             .stop { color: blue }
           }
           </style>
           <p id=outside>outside</p>
           <section class=card>
             <p id=inside>inside</p>
             <div class=stop><p id=blocked>blocked</p></div>
             <p id=after>after</p>
           </section>"#,
    );
    let styles = StyleSet::from_dom(&dom, &[], 800.0);
    assert_eq!(color(&dom, &styles, "outside"), Color::rgb(0, 0, 0));
    assert_eq!(color(&dom, &styles, "inside"), Color::rgb(255, 0, 0));
    assert_eq!(color(&dom, &styles, "blocked"), Color::rgb(0, 0, 0));
    assert_eq!(color(&dom, &styles, "after"), Color::rgb(255, 0, 0));
}

#[test]
fn nested_scopes_resolve_inner_roots_against_outer_roots() {
    let dom = dom::parse(
        r#"<style>
           p { color: black }
           @scope (.outer) {
             p { color: red }
             @scope (:scope > .inner) {
               p { color: blue }
               :scope { color: green }
             }
           }
           </style>
           <div class=inner><p id=unrelated>unrelated</p></div>
           <div class=outer>
             <p id=outer>outer</p>
             <div class=inner id=inner><p id=nested>nested</p></div>
           </div>"#,
    );
    let styles = StyleSet::from_dom(&dom, &[], 800.0);
    assert_eq!(color(&dom, &styles, "unrelated"), Color::rgb(0, 0, 0));
    assert_eq!(color(&dom, &styles, "outer"), Color::rgb(255, 0, 0));
    assert_eq!(color(&dom, &styles, "inner"), Color::rgb(0, 128, 0));
    assert_eq!(color(&dom, &styles, "nested"), Color::rgb(0, 0, 255));
}

#[test]
fn scope_nested_in_style_rule_uses_the_parent_selector_as_its_start_context() {
    let dom = dom::parse(
        r#"<style>
           p { color: black }
           .outer {
             @scope (.inner) {
               p { color: red }
             }
           }
           </style>
           <div class=inner><p id=unrelated>unrelated</p></div>
           <div class=outer>
             <p id=outside>outside</p>
             <div class=inner><p id=inside>inside</p></div>
           </div>"#,
    );
    let styles = StyleSet::from_dom(&dom, &[], 800.0);
    assert_eq!(color(&dom, &styles, "unrelated"), Color::rgb(0, 0, 0));
    assert_eq!(color(&dom, &styles, "outside"), Color::rgb(0, 0, 0));
    assert_eq!(color(&dom, &styles, "inside"), Color::rgb(255, 0, 0));
}

#[test]
fn scope_pseudo_and_relative_selectors_use_the_current_root() {
    let dom = dom::parse(
        r#"<style>
           p { color: black }
           @scope (#component) {
             :scope { color: red }
             > p { color: blue }
             & > p.special { color: green }
           }
           </style>
           <section id=component>
             <p id=direct>direct</p>
             <p class=special id=special>special</p>
             <div><p id=deep>deep</p></div>
           </section>"#,
    );
    let styles = StyleSet::from_dom(&dom, &[], 800.0);
    assert_eq!(color(&dom, &styles, "component"), Color::rgb(255, 0, 0));
    assert_eq!(color(&dom, &styles, "direct"), Color::rgb(0, 0, 255));
    assert_eq!(color(&dom, &styles, "special"), Color::rgb(0, 128, 0));
    assert_eq!(color(&dom, &styles, "deep"), Color::rgb(0, 0, 0));
}

#[test]
fn proximity_precedes_source_order_but_follows_specificity() {
    let dom = dom::parse(
        r#"<style>
           @scope (.near) { p { color: red } }
           @scope (.far) { p { color: blue } }
           @scope (.far) { #specific { color: purple } }
           @scope (.near) { p.important { background-color: red !important } }
           @scope (.far) { p.important { background-color: blue !important } }
           </style>
           <div class=far><div class=near>
             <p id=ordinary>ordinary</p>
             <p id=specific>specific</p>
             <p class=important id=important>important</p>
           </div></div>"#,
    );
    let styles = StyleSet::from_dom(&dom, &[], 800.0);
    assert_eq!(color(&dom, &styles, "ordinary"), Color::rgb(255, 0, 0));
    assert_eq!(color(&dom, &styles, "specific"), Color::rgb(128, 0, 128));
    let important = Node::descendants(&dom.document)
        .find(|node| node.attr_ref("id").as_deref() == Some("important"))
        .unwrap();
    assert_eq!(
        styles.get(&important).background_color,
        Color::rgb(255, 0, 0)
    );
}

#[test]
fn omitted_start_uses_the_stylesheet_owner_parent() {
    let dom = dom::parse(
        r#"<p id=before>before</p>
           <section>
             <style>@scope { p { color: red } }</style>
             <p id=inside>inside</p>
           </section>
           <p id=after>after</p>"#,
    );
    let styles = StyleSet::from_dom(&dom, &[], 800.0);
    assert_eq!(color(&dom, &styles, "before"), Color::rgb(0, 0, 0));
    assert_eq!(color(&dom, &styles, "inside"), Color::rgb(255, 0, 0));
    assert_eq!(color(&dom, &styles, "after"), Color::rgb(0, 0, 0));
}

#[test]
fn document_adopted_implicit_scope_does_not_use_html_as_a_virtual_root() {
    let dom = dom::parse("<p id=target>target</p>");
    dom.document
        .set_adopted_stylesheets(vec![crate::engine::AdoptedStyleSheet {
            source: "@scope { :scope { color: red } p { color: red } }".into(),
            media: String::new(),
            base_url: String::new(),
        }]);
    let styles = StyleSet::from_dom(&dom, &[], 800.0);
    let html = dom.elements_named("html").next().unwrap();
    assert_eq!(styles.get(&html).color, Color::rgb(0, 0, 0));
    assert_eq!(color(&dom, &styles, "target"), Color::rgb(0, 0, 0));
}

#[test]
fn document_adopted_explicit_scopes_keep_inheritance_and_proximity() {
    let dom = dom::parse(
        "<section id=near><span id=inherited>inherited</span><p id=inside>inside</p></section><p id=outside>outside</p>",
    );
    dom.document
        .set_adopted_stylesheets(vec![crate::engine::AdoptedStyleSheet {
        source:
            "@scope (#near) { :scope { background-color: red; color: green } p { color: green } } \
                     @scope (html) { p { color: blue } }"
                .into(),
        media: String::new(),
        base_url: String::new(),
    }]);
    let styles = StyleSet::from_dom(&dom, &[], 800.0);
    let near = Node::descendants(&dom.document)
        .find(|node| node.attr_ref("id").as_deref() == Some("near"))
        .unwrap();
    assert_eq!(styles.get(&near).color, Color::rgb(0, 128, 0));
    assert_eq!(color(&dom, &styles, "inherited"), Color::rgb(0, 128, 0));
    assert_eq!(color(&dom, &styles, "inside"), Color::rgb(0, 128, 0));
    assert_eq!(color(&dom, &styles, "outside"), Color::rgb(0, 0, 255));
}

#[test]
fn nested_omitted_start_must_still_be_inside_the_outer_scope() {
    let dom = dom::parse(
        r#"<body>
           <style>
             @scope (.outer) {
               @scope { #outside-owner { color: red } }
               @scope (:scope) { #explicit-inner { color: blue } }
             }
           </style>
           <section class=outer>
             <p id=outside-owner>outside owner</p>
             <p id=explicit-inner>explicit inner</p>
             <style>@scope (.outer) { @scope { #inside-owner { color: purple } } }</style>
             <p id=inside-owner>inside owner</p>
           </section></body>"#,
    );
    let styles = StyleSet::from_dom(&dom, &[], 800.0);
    assert_eq!(color(&dom, &styles, "outside-owner"), Color::rgb(0, 0, 0));
    assert_eq!(
        color(&dom, &styles, "explicit-inner"),
        Color::rgb(0, 0, 255)
    );
    assert_eq!(
        color(&dom, &styles, "inside-owner"),
        Color::rgb(128, 0, 128)
    );
}

#[test]
fn scope_nested_in_style_rule_rejects_an_unsupported_boundary_member() {
    let dom = dom::parse(
        r#"<style>
           .outer {
             @scope (.inner, :unsupported-pseudo) {
               #invalid { color: red }
             }
             @scope (.inner) {
               #valid { color: blue }
             }
           }
           </style>
           <div class=outer><div class=inner>
             <p id=invalid>invalid scope</p>
             <p id=valid>valid scope</p>
           </div></div>"#,
    );
    let styles = StyleSet::from_dom(&dom, &[], 800.0);
    assert_eq!(color(&dom, &styles, "invalid"), Color::rgb(0, 0, 0));
    assert_eq!(color(&dom, &styles, "valid"), Color::rgb(0, 0, 255));
}

#[test]
fn scoped_style_rule_selector_lists_are_unforgiving() {
    let dom = dom::parse(
        r#"<style>
           @scope (.outer) {
             #invalid, :unsupported-pseudo { color: red }
             #valid, :focus-visible { color: blue }
           }
           </style>
           <div class=outer>
             <p id=invalid>invalid list</p>
             <p id=valid>valid list with nonmatching pseudo</p>
           </div>"#,
    );
    let styles = StyleSet::from_dom(&dom, &[], 800.0);
    assert_eq!(color(&dom, &styles, "invalid"), Color::rgb(0, 0, 0));
    assert_eq!(color(&dom, &styles, "valid"), Color::rgb(0, 0, 255));
}

#[test]
fn deeply_overlapping_scope_chains_choose_the_nearest_innermost_root() {
    let mut html = String::from("<style>");
    for _ in 0..6 {
        html.push_str("@scope (.x) {");
    }
    html.push_str("p { color: red }");
    for _ in 0..6 {
        html.push('}');
    }
    html.push_str("@scope (#far) { p { color: blue } }</style><div class=x id=far>");
    for _ in 1..32 {
        html.push_str("<div class=x>");
    }
    html.push_str("<p id=target>target</p>");
    for _ in 0..32 {
        html.push_str("</div>");
    }
    let dom = dom::parse(&html);
    let styles = StyleSet::from_dom(&dom, &[], 800.0);
    assert_eq!(color(&dom, &styles, "target"), Color::rgb(255, 0, 0));
}

#[test]
fn direct_scope_declarations_match_only_roots_with_zero_specificity() {
    let dom = dom::parse(
        r#"<style>
           #strong { color: blue }
           @scope (#simple) { color: red }
           @scope (#strong) { color: red }
           @scope (.outer) {
             @scope (.inner) { color: green }
           }
           </style>
           <div id=simple><p id=descendant>descendant</p></div>
           <div id=strong>strong</div>
           <section class=outer id=outer>
             <div class=inner id=inner>inner</div>
           </section>"#,
    );
    let styles = StyleSet::from_dom(&dom, &[], 800.0);
    assert_eq!(color(&dom, &styles, "simple"), Color::rgb(255, 0, 0));
    assert_eq!(color(&dom, &styles, "strong"), Color::rgb(0, 0, 255));
    assert_eq!(color(&dom, &styles, "outer"), Color::rgb(0, 0, 0));
    assert_eq!(color(&dom, &styles, "inner"), Color::rgb(0, 128, 0));
}

#[test]
fn mixed_scope_declaration_runs_keep_their_rule_order() {
    let dom = dom::parse(
        r#"<style>
           @scope (#mixed) {
             color: red;
             :where(:scope) { color: blue }
             color: green;
           }
           @scope (#grouped) {
             color: blue;
             @media screen { :where(:scope) { color: green } }
             color: red;
           }
           </style>
           <div id=mixed>mixed</div>
           <div id=grouped>grouped</div>"#,
    );
    let styles = StyleSet::from_dom(&dom, &[], 800.0);
    assert_eq!(color(&dom, &styles, "mixed"), Color::rgb(0, 128, 0));
    assert_eq!(color(&dom, &styles, "grouped"), Color::rgb(255, 0, 0));
}

#[test]
fn direct_declarations_in_style_nested_scope_target_the_new_root() {
    let dom = dom::parse(
        r#"<style>.outer { @scope (.inner) { color: red; } }</style>
           <div class=outer id=outer>
             <div class=inner id=inner>inner</div>
           </div>"#,
    );
    let styles = StyleSet::from_dom(&dom, &[], 800.0);
    assert_eq!(color(&dom, &styles, "outer"), Color::rgb(0, 0, 0));
    assert_eq!(color(&dom, &styles, "inner"), Color::rgb(255, 0, 0));
}
