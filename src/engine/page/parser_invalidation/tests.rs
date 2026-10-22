use super::*;
use crate::engine::dom::incremental::{HtmlParser, ParserStep};

fn run(parser: &mut HtmlParser) {
    while !matches!(parser.advance(), ParserStep::NeedInput | ParserStep::End) {}
}

/// Parses `first`, styles it, streams `rest`, and refreshes only the parser's roots.
/// Every element must then match a from-scratch cascade of the final document.
fn stream(first: &str, rest: &str) -> (Page, RenderInvalidation, StyleRefreshStats) {
    let mut parser = HtmlParser::streaming();
    parser.dom().retain_parser_render_changes();
    parser.append(first, false).unwrap();
    run(&mut parser);
    let mut page = Page::from_dom(parser.dom().clone(), "https://example.test/");
    page.refresh_resources(800.0);
    page.dom.take_parser_render_changes();
    parser.append(rest, true).unwrap();
    run(&mut parser);
    let changes = page.dom.take_parser_render_changes();
    let invalidation = page.parser_render_invalidation(&changes);
    let stats = page.refresh_resources_after_invalidation(800.0, &invalidation);
    let fresh = page.style(800.0);
    let incremental = page.cached_style(800.0).expect("refreshed styles");
    for node in Node::descendants(&page.dom.document).filter(|node| node.element().is_some()) {
        assert!(
            incremental.styles.get(&node.id()) == fresh.styles.get(&node.id()),
            "stale style for <{}> after streaming {rest:?}",
            node.tag_name().unwrap_or_default(),
        );
    }
    (page, invalidation, stats)
}

fn scoped(invalidation: &RenderInvalidation, page: &Page) -> bool {
    !invalidation.is_empty() && !invalidation.roots.contains(&page.dom.document.id())
}

#[test]
fn appended_children_restyle_only_new_subtrees_without_trailing_selectors() {
    let first = "<style>p{color:blue} p+p{color:green}</style><main><p>one</p><p>two</p>";
    let (page, invalidation, stats) = stream(first, "<p>three<b>bold</b></p></main>");
    assert!(scoped(&invalidation, &page));
    let main = page.dom.elements_named("main").next().unwrap();
    assert!(!invalidation.roots.contains(&main.id()));
    assert!(
        stats.recomputed_styles < stats.total_styles / 2,
        "{stats:?}"
    );
    assert!(!invalidation.rebuild_style_rules);
}

#[test]
fn trailing_structural_selectors_restyle_previous_siblings() {
    for (style, rest) in [
        ("li:last-child{color:red}", "<li>c</li></ul>"),
        ("li:only-child{color:red}", "<li>c</li></ul>"),
        ("li:last-of-type{color:red}", "<p>x</p><li>c</li></ul>"),
        ("li:only-of-type{color:red}", "<p>x</p><li>c</li></ul>"),
        (
            "li:nth-last-child(3){color:red}",
            "<li>c</li><li>d</li></ul>",
        ),
        ("li:nth-last-of-type(2){color:red}", "<li>c</li></ul>"),
        ("li:is(:not(:last-child)) b{color:red}", "<li>c</li></ul>"),
    ] {
        let first = format!("<style>{style}</style><ul><li>a<b>x</b></li><li>b<b>y</b></li>");
        let (page, invalidation, _) = stream(&first, rest);
        assert!(scoped(&invalidation, &page), "{style}");
    }
}

#[test]
fn previous_last_children_restyle_as_elements_unless_a_condition_reaches_below() {
    let style = concat!(
        "<style>div:last-child{margin-left:3px} section:last-child{color:red}",
        "li:last-child b{color:blue} aside.deep:last-child p{color:green}",
        "main:has(> div:last-child) p{color:blue}",
        "</style>",
    );
    for (previous, reaches_below) in [
        ("<div><p>a</p><p>b</p></div>", false),
        ("<section><p>a</p></section>", false),
        ("<aside class=deep><p>a</p></aside>", true),
    ] {
        let first = format!("{style}<main>{previous}");
        let (page, invalidation, _) = stream(&first, "<span>new</span></main>");
        let main = page.dom.elements_named("main").next().unwrap();
        let previous = main.children.borrow()[0].clone();
        assert!(invalidation.roots.contains(&previous.id()), "{first}");
        assert_eq!(
            invalidation.element_roots.contains(&previous.id()),
            !reaches_below,
            "{first}"
        );
    }
}

#[test]
fn parent_dependencies_widen_to_the_parent() {
    for (style, first, rest) in [
        ("div:empty{color:red}", "<div> ", "text</div>"),
        (
            "div:empty{color:red}",
            "<div><!-- c -->",
            "<span></span></div>",
        ),
        (
            "ul:has(> li.x){color:red}",
            "<ul><li>a</li>",
            "<li class=x>b</li></ul>",
        ),
        (
            "li:has(+ .x){color:red}",
            "<ul><li>a</li>",
            "<li class=x>b</li></ul>",
        ),
    ] {
        let (page, invalidation, _) = stream(&format!("<style>{style}</style>{first}"), rest);
        assert!(!invalidation.is_empty(), "{style}");
        let _ = page;
    }
}

#[test]
fn foster_parenting_and_stylesheets_keep_conservative_scope() {
    // A foster-parented node is inserted before the table, shifting later siblings.
    let first = "<style>table+b{color:red} b+table{color:blue}</style><div><table><tr><td>x</td>";
    let (page, invalidation, _) = stream(first, "<b>foster</b></td></tr></table></div>");
    let div = page.dom.elements_named("div").next().unwrap();
    assert!(scoped(&invalidation, &page));
    let _ = div;
    let (_, invalidation, _) = stream("<main><p>a</p>", "<style>p{color:red}</style></main>");
    assert!(invalidation.rebuild_style_rules);
}

#[test]
fn document_metadata_and_ungrouped_radios_restyle_everything() {
    for (first, rest) in [
        ("<head>", "<base href='/other/'></head><body>"),
        (
            "<head>",
            "<meta name=color-scheme content=dark></head><body>",
        ),
        (
            "<body><input type=radio name=r checked>",
            "<input type=radio name=r checked>",
        ),
    ] {
        let (page, invalidation, _) = stream(first, rest);
        assert_eq!(invalidation.roots, vec![page.dom.document.id()], "{rest}");
    }
    let (page, invalidation, _) = stream(
        "<body><form><input type=radio name=r checked>",
        "<input type=radio name=r checked></form>",
    );
    let form = page.dom.elements_named("form").next().unwrap();
    assert!(invalidation.roots.contains(&form.id()));
}

#[test]
fn inert_template_contents_produce_no_rendering_work() {
    let (_, invalidation, _) = stream("<body><template>", "<p>inert</p>");
    assert!(invalidation.is_empty());
}

#[test]
fn declarative_shadow_content_restyles_from_its_host() {
    let first = "<body><div id=host><template shadowrootmode=open><slot></slot>";
    let (page, invalidation, _) = stream(first, "<p>shadow</p></template><span>light</span></div>");
    let host = page.dom.elements_named("div").next().unwrap();
    assert!(invalidation.roots.contains(&host.id()));
}
