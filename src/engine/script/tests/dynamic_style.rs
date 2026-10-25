//! Computed values and selector state that must follow script-driven changes.

fn status_after_script(body: &str) -> String {
    let html = format!(r#"<body>{body}<output id="status"></output></body>"#);
    let (dom, outcome) = super::execute_html(&html);
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    dom.elements_named("output").next().unwrap().text_content()
}

#[test]
fn white_space_resolves_to_its_keyword_and_invalid_later_declarations_are_dropped() {
    let status = status_after_script(
        r#"<style>p { white-space: pre-wrap; white-space: x-bogus; }</style><p id="p"></p>
        <script>document.getElementById('status').textContent =
          getComputedStyle(document.getElementById('p')).whiteSpace + ' ' +
          getComputedStyle(document.body).whiteSpace;</script>"#,
    );
    assert_eq!(status, "pre-wrap normal");
}

#[test]
fn cursor_computes_to_its_keyword_inherits_and_ignores_unknown_keywords() {
    // https://drafts.csswg.org/css-ui-4/#cursor
    let status = status_after_script(
        r#"<style>#a { cursor: nesw-resize; } #b { cursor: bogus; }</style>
        <div id="a"><span id="c"></span></div><div id="b"></div>
        <script>const cursor = id => getComputedStyle(document.getElementById(id)).cursor;
          document.getElementById('status').textContent =
            [cursor('a'), cursor('c'), cursor('b')].join(' ');</script>"#,
    );
    assert_eq!(status, "nesw-resize nesw-resize auto");
}

#[test]
fn empty_pseudo_class_follows_character_data_changes() {
    // White-space-only text defeats :empty in every shipping engine (Selectors 3).
    // https://www.w3.org/TR/selectors-3/#empty-pseudo
    let status = status_after_script(
        r#"<style>p:empty { text-transform: uppercase; }</style><p id="p"></p>
        <script>const p = document.getElementById('p');
          const result = [];
          const record = () => result.push(getComputedStyle(p).textTransform);
          p.appendChild(document.createComment('c'));
          p.appendChild(document.createTextNode(''));
          record();
          p.lastChild.data = ' ';
          record();
          p.lastChild.data = '';
          record();
          document.getElementById('status').textContent = result.join(' ');</script>"#,
    );
    assert_eq!(status, "uppercase none uppercase");
}

#[test]
fn style_element_text_changes_restyle_the_document() {
    // https://html.spec.whatwg.org/multipage/semantics.html#update-a-style-block
    let status = status_after_script(
        r#"<style id="s">p { text-transform: uppercase; }</style><p id="p"></p>
        <script>const p = document.getElementById('p'), s = document.getElementById('s');
          const result = [getComputedStyle(p).textTransform];
          s.firstChild.data = 'p { text-transform: lowercase; }';
          result.push(getComputedStyle(p).textTransform);
          s.appendChild(document.createTextNode('p { text-transform: capitalize; }'));
          result.push(getComputedStyle(p).textTransform);
          document.getElementById('status').textContent = result.join(' ');</script>"#,
    );
    assert_eq!(status, "uppercase lowercase capitalize");
}

#[test]
fn script_observes_visited_links_as_unvisited() {
    // Selectors 4 permits, and browsers require, that history is not observable from
    // computed styles or selector APIs. https://drafts.csswg.org/selectors-4/#link
    use crate::engine::css::selector_match::visited;
    visited::clear_visited_urls();
    visited::record_visited_url("https://example.com/seen");
    let status = status_after_script(
        r#"<style>a:link { color: red } a:visited { color: white }</style><a id="a" href="seen">x</a>
        <script>const a = document.getElementById('a');
          document.getElementById('status').textContent = [getComputedStyle(a).color,
            a.matches(':visited'), a.matches(':link'), document.querySelector(':visited')].join(' ');</script>"#,
    );
    visited::clear_visited_urls();
    assert_eq!(status, "rgb(255, 0, 0) false true ");
}

#[test]
fn font_shorthand_with_small_caps_applies_and_resets_line_height() {
    // https://drafts.csswg.org/css-fonts-4/#font-prop
    let status = status_after_script(
        r#"<style>* { line-height: 1.2 } #a { font: 900 small-caps 10px sans-serif }</style>
        <p id="a"></p><script>const style = getComputedStyle(document.getElementById('a'));
          document.getElementById('status').textContent =
            [style.fontSize, style.fontWeight, style.lineHeight, style.fontVariantCaps].join(' ');</script>"#,
    );
    assert_eq!(status, "10px 900 normal small-caps");
}
