//! SVG font attributes are author presentation values, not declaration lists.
//! Resolve after custom properties but before all author sheets (including layers).
//! https://svgwg.org/svg2-draft/styling.html#PresentationAttributes
use super::*;
use cssparser::Token;

pub(super) fn apply(node: &NodeRef, style: &mut ComputedStyle, context: DeclarationContext<'_>) {
    if node.namespace_uri() != Some("http://www.w3.org/2000/svg") {
        return;
    }
    for name in [
        "font-family",
        "font-size",
        "font-weight",
        "font-style",
        "letter-spacing",
        "word-spacing",
    ] {
        let Some(value) = node.attr(name) else {
            continue;
        };
        let Some(value) =
            super::super::variables::substitute_variables(&value, &style.custom_properties)
        else {
            continue;
        };
        let Some(value) = value_for_property(name, &value) else {
            continue;
        };
        let declarations = parse_declarations(&format!("{name}:{value}"));
        if declarations.len() != 1 || declarations[0].name != name || declarations[0].important {
            continue;
        }
        super::super::variables::apply_resolved_declaration(
            style,
            &declarations[0],
            DeclarationContext {
                parent: context.parent,
                lower_origin: context.lower_origin,
                layer_start: context.layer_start,
                base_url: context.base_url,
                viewport_width: context.viewport_width,
                viewport_height: context.viewport_height,
            },
        );
    }
}

fn value_for_property(name: &str, value: &str) -> Option<String> {
    if value.len() > crate::limits::MAX_CSS_SOURCE_BYTES {
        return None;
    }
    let mut input = ParserInput::new(value);
    let mut parser = Parser::new(&mut input);
    while !parser.is_exhausted() {
        match parser.next().ok()? {
            Token::Semicolon | Token::Delim('!') | Token::CurlyBracketBlock => return None,
            _ => {}
        }
    }
    if matches!(name, "font-size" | "letter-spacing" | "word-spacing")
        && let Ok(number) = value.trim().parse::<f32>()
    {
        if !number.is_finite() || (name == "font-size" && number < 0.0) {
            return None;
        }
        return Some(format!("{number}px"));
    }
    Some(value.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn style(html: &str, id: &str) -> ComputedStyle {
        let dom = crate::engine::dom::parse(html);
        let node = Node::descendants(&dom.document)
            .find(|node| node.attr("id").as_deref() == Some(id))
            .unwrap();
        StyleSet::from_dom(&dom, &[], 800.0).get(&node).clone()
    }

    #[test]
    fn svg_font_attributes_follow_author_cascade_and_rollback_origins() {
        let html = r#"<style>.parent{font-size:30px}.rule{font-size:20px}
            @layer base{.layer{font-size:20px}} .revert{font-size:revert}
            @layer rollback{.rollback{font-size:revert-layer}}</style>
            <div class=parent><svg><text id=attribute font-size=40>A</text>
            <text id=rule class=rule font-size=40>A</text><text id=layer class=layer font-size=40>A</text>
            <text id=revert class=revert font-size=40>A</text><text id=rollback class=rollback font-size=40>A</text></svg></div>"#;
        // Independently captured Chromium 154 values in presentation-fonts.html.
        for (id, size) in [
            ("attribute", 40.0),
            ("rule", 20.0),
            ("layer", 20.0),
            ("revert", 30.0),
            ("rollback", 40.0),
        ] {
            assert_eq!(style(html, id).font_size, size, "{id}");
        }
    }

    #[test]
    fn svg_font_attributes_inherit_and_resolve_current_custom_properties() {
        let computed = style(
            r#"<style>#target{--size:2em;--family:'SVG Alias',serif}</style>
            <svg font-size=20 font-weight=700><text id=target font-size="var(--size)" font-family="var(--family)" letter-spacing=2 word-spacing=-1>T</text></svg>"#,
            "target",
        );
        assert_eq!(computed.font_size, 40.0);
        assert_eq!(computed.font_weight, 700);
        assert!(computed.font_family.contains("SVG Alias"));
        assert_eq!(computed.letter_spacing, 2.0);
        assert_eq!(computed.word_spacing, -1.0);
    }

    #[test]
    fn presentation_values_cannot_inject_declarations_or_importance() {
        for value in [
            "40 !important",
            "40;",
            "40;display:none",
            "calc(40px);font-weight:900",
        ] {
            let html = format!("<svg><text id=target font-size=\"{value}\">T</text></svg>");
            let computed = style(&html, "target");
            assert_eq!(
                computed.font_size,
                ComputedStyle::initial().font_size,
                "{value}"
            );
            assert_ne!(computed.display, Display::None);
        }
        assert_eq!(
            style("<div id=target font-size=40>T</div>", "target").font_size,
            ComputedStyle::initial().font_size
        );
    }

    #[test]
    fn svg_presentation_attributes_do_not_apply_to_generated_pseudo_elements() {
        let dom = crate::engine::dom::parse(
            "<svg font-size=20><text id=target font-size=2em>T</text></svg>",
        );
        let node = Node::descendants(&dom.document)
            .find(|node| node.attr("id").as_deref() == Some("target"))
            .unwrap();
        let mut styles = StyleSet::from_dom(&dom, &[], 800.0);
        let computed = styles
            .computed_style_for_pseudo(&node, PseudoElement::Before)
            .unwrap();
        assert_eq!(
            computed.font_size, 40.0,
            "pseudo inherits the origin's font, not a second relative attribute application"
        );
    }
}
