//! Keep shaping work bounded before a native SVG tree resolves font outlines.
//! The byte/image budgets alone do not bound a tiny bitmap with enormous text.
use resvg::usvg::roxmltree::{Document, ParsingOptions};

const MAX_TEXT_CHARACTERS: usize = 16_384;
const MAX_TEXT_LENGTH_ELEMENTS: usize = 256;
// Native text/tree conversion is recursive. Keep admitted documents within a
// conservative normal renderer/test-thread stack budget, not just an XML limit.
const MAX_DEPTH: usize = 64;

pub(super) fn document(source: &[u8]) -> Result<Document<'_>, String> {
    let text = std::str::from_utf8(source).map_err(|_| "SVG source is not UTF-8")?;
    preflight_depth(text)?;
    let document = Document::parse_with_options(
        text,
        ParsingOptions {
            allow_dtd: true,
            nodes_limit: crate::limits::MAX_DOM_NODES as u32,
            ..Default::default()
        },
    )
    .map_err(|error| format!("parse SVG XML: {error}"))?;
    let mut characters = 0;
    let mut lengths = 0;
    for node in document.descendants() {
        if node
            .ancestors()
            .filter(|ancestor| ancestor.is_element())
            .take(MAX_DEPTH + 1)
            .count()
            > MAX_DEPTH
        {
            return Err("SVG exceeds the structural depth budget".into());
        }
        if node.is_element()
            && matches!(node.tag_name().name(), "text" | "tspan")
            && node.attribute("textLength").is_some()
        {
            lengths += 1;
            if lengths > MAX_TEXT_LENGTH_ELEMENTS {
                return Err("SVG exceeds the text-length element budget".into());
            }
        }
        if node.is_text()
            && node
                .ancestors()
                .any(|ancestor| ancestor.is_element() && ancestor.tag_name().name() == "text")
        {
            characters += node.text().unwrap_or_default().chars().count();
            if characters > MAX_TEXT_CHARACTERS {
                return Err("SVG exceeds the text shaping character budget".into());
            }
        }
    }
    Ok(document)
}

fn preflight_depth(source: &str) -> Result<(), String> {
    // Reuse the existing zero-allocation XML tokenizer before roxmltree's
    // recursive tree construction. Counting '<' bytes would misread comments,
    // CDATA and quoted attributes, and a post-parse depth check is too late.
    let mut depth = 0_usize;
    for token in xmlparser::Tokenizer::from(source) {
        match token.map_err(|error| format!("tokenize SVG XML: {error}"))? {
            xmlparser::Token::ElementStart { .. } => {
                depth += 1;
                if depth > MAX_DEPTH {
                    return Err("SVG exceeds the structural depth budget".into());
                }
            }
            xmlparser::Token::ElementEnd {
                end: xmlparser::ElementEnd::Close(..) | xmlparser::ElementEnd::Empty,
                ..
            } => {
                depth = depth.checked_sub(1).ok_or("unbalanced SVG XML element")?;
            }
            // Embedded markup must not bypass the pre-parse structural bound.
            // Ordinary SVG DOCTYPEs and text-only internal entities remain valid.
            xmlparser::Token::EntityDeclaration {
                definition: xmlparser::EntityDefinition::EntityValue(value),
                ..
            } if value.as_str().contains('<') => {
                return Err("SVG markup entities are not supported by the bounded decoder".into());
            }
            _ => {}
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn svg(content: &str) -> String {
        format!("<svg xmlns='http://www.w3.org/2000/svg' width='1' height='1'>{content}</svg>")
    }

    #[test]
    fn small_images_do_not_bypass_the_text_work_budget() {
        let accepted = svg(&format!("<text>{}</text>", "x".repeat(MAX_TEXT_CHARACTERS)));
        assert!(document(accepted.as_bytes()).is_ok());
        let rejected = svg(&format!(
            "<text>{}</text>",
            "x".repeat(MAX_TEXT_CHARACTERS + 1)
        ));
        assert!(
            document(rejected.as_bytes())
                .unwrap_err()
                .contains("character budget")
        );
    }

    #[test]
    fn budget_counts_scalars_and_expanded_xml_text_not_encoded_bytes() {
        let source = svg("<text>é&#x1F600;<tspan>&amp;</tspan></text>");
        assert!(document(source.as_bytes()).is_ok());
        let source = svg(&format!(
            "<text>{}</text>",
            "&#65;".repeat(MAX_TEXT_CHARACTERS + 1)
        ));
        assert!(document(source.as_bytes()).is_err());
    }

    #[test]
    fn adjustment_and_depth_budgets_apply_before_native_layout() {
        let source = svg(&format!(
            "<text>{}</text>",
            "<tspan textLength='1'>a</tspan>".repeat(MAX_TEXT_LENGTH_ELEMENTS + 1)
        ));
        assert!(
            document(source.as_bytes())
                .unwrap_err()
                .contains("element budget")
        );
        let source = svg(&format!(
            "{}<text>x</text>{}",
            "<g>".repeat(MAX_DEPTH + 1),
            "</g>".repeat(MAX_DEPTH + 1)
        ));
        assert!(
            document(source.as_bytes())
                .unwrap_err()
                .contains("depth budget")
        );
    }

    #[test]
    fn invalid_utf8_and_malformed_xml_fail_without_font_selection() {
        assert!(document(&[255]).is_err());
        assert!(document(b"<svg><text></svg>").is_err());
    }

    #[test]
    fn comments_cdata_and_quoted_markup_do_not_count_as_elements() {
        let source = svg("<!-- <g><g> --><g data-note='&lt;g>'/><text><![CDATA[<g>]]></text>");
        assert!(document(source.as_bytes()).is_ok());
    }

    #[test]
    fn accepted_depth_boundary_parses_on_a_normal_test_thread() {
        let source = svg(&format!(
            "{}x{}",
            "<g>".repeat(MAX_DEPTH - 1),
            "</g>".repeat(MAX_DEPTH - 1)
        ));
        assert!(document(source.as_bytes()).is_ok());
    }

    #[test]
    fn markup_entities_cannot_bypass_preparse_depth_but_text_entities_work() {
        let markup = "<!DOCTYPE svg [<!ENTITY inner '<g/>'>]><svg xmlns='http://www.w3.org/2000/svg'>&inner;</svg>";
        assert!(
            document(markup.as_bytes())
                .unwrap_err()
                .contains("markup entities")
        );
        let text = "<!DOCTYPE svg [<!ENTITY label 'SVG'>]><svg xmlns='http://www.w3.org/2000/svg'><text>&label;</text></svg>";
        assert!(document(text.as_bytes()).is_ok());
    }
}
