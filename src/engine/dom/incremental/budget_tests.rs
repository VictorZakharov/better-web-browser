use super::*;
use crate::engine::dom::Node;
use crate::limits::MAX_DOM_DEPTH;

/// Streams `source` in network-sized chunks until the parser ends. A parser that truncates the
/// tree stops reading input, as it does for a navigation.
fn stream(source: &str) -> HtmlParser {
    let mut parser = HtmlParser::streaming();
    let chunks = source.as_bytes().chunks(1024).collect::<Vec<_>>();
    for (index, chunk) in chunks.iter().enumerate() {
        let text = std::str::from_utf8(chunk).unwrap();
        parser.append(text, index + 1 == chunks.len()).unwrap();
        loop {
            match parser.advance() {
                ParserStep::NeedInput => break,
                ParserStep::End => return parser,
                _ => {}
            }
        }
    }
    parser
}

/// The depth of the deepest node, counting template contents below their template as
/// `budget::enforce` does.
fn deepest(root: &NodeRef, depth: usize) -> usize {
    let mut deepest_node = depth;
    for child in root.children.borrow().iter() {
        deepest_node = deepest_node.max(deepest(child, depth + 1));
    }
    if let Some(contents) = root
        .element()
        .and_then(|element| element.template_contents.borrow().clone())
    {
        deepest_node = deepest_node.max(deepest(&contents, depth + 1));
    }
    deepest_node
}

fn truncated(parser: &HtmlParser) -> bool {
    parser
        .dom()
        .errors
        .borrow()
        .iter()
        .any(|error| error.starts_with("safety limit: DOM was truncated"))
}

#[test]
fn streamed_nesting_beyond_the_depth_budget_is_pruned() {
    let source = format!("<main>{}payload", "<div>".repeat(MAX_DOM_DEPTH + 64));
    let parser = stream(&source);
    assert!(deepest(&parser.dom().document, 0) <= MAX_DOM_DEPTH);
    assert!(truncated(&parser));
}

#[test]
fn template_contents_beyond_the_depth_budget_are_pruned() {
    let source = format!(
        "<div><template>{}payload</template></div><p>after",
        "<span>".repeat(MAX_DOM_DEPTH + 64)
    );
    let parser = stream(&source);
    assert!(deepest(&parser.dom().document, 0) <= MAX_DOM_DEPTH);
    assert!(truncated(&parser));
}

#[test]
fn adoption_agency_moves_stay_within_the_depth_budget() {
    // `</a>` moves the open `<p>` out of `<a>` and its descendants into a new `<a>` below it,
    // which reparents a subtree whose deepest text sits at the depth budget.
    let source = format!(
        "{}<a><p>{}x</a>after",
        "<div>".repeat(MAX_DOM_DEPTH - 12),
        "<span>".repeat(7)
    );
    let parser = stream(&source);
    assert!(deepest(&parser.dom().document, 0) <= MAX_DOM_DEPTH);
}

#[test]
fn documents_within_the_budget_are_not_truncated() {
    let source = format!(
        "<main>{}<p>end</p>",
        "<section><p>text <b>bold</b></p></section>".repeat(2_000)
    );
    let parser = stream(&source);
    assert!(!truncated(&parser));
    assert!(!parser.dom().parser_depth_check.get());
    assert_eq!(
        Node::descendants(&parser.dom().document)
            .filter(|node| node.tag_name() == Some("section"))
            .count(),
        2_000
    );
}
