//! Cascade integration for constructed sheets adopted by a document or shadow root.

use super::*;

pub(super) fn collect(
    document: &NodeRef,
    document_base_url: &str,
    external_stylesheets: &[(String, String)],
    viewport_width: f32,
    prefers_dark_color_scheme: bool,
) -> Vec<Rule> {
    let mut rules = Vec::new();
    let mut next_order = 0_u32;
    for style_element in Node::descendants(document).filter(|node| node.tag_name() == Some("style"))
    {
        parse_stylesheet(
            &style_element.text_content(),
            document_base_url,
            viewport_width,
            prefers_dark_color_scheme,
            &mut next_order,
            &mut rules,
            RuleScope::Document,
        );
    }
    for (source_url, stylesheet) in external_stylesheets {
        parse_stylesheet(
            stylesheet,
            source_url,
            viewport_width,
            prefers_dark_color_scheme,
            &mut next_order,
            &mut rules,
            RuleScope::Document,
        );
    }
    append_adopted(
        document,
        viewport_width,
        prefers_dark_color_scheme,
        &mut next_order,
        &mut rules,
        RuleScope::Document,
    );
    for shadow in Node::shadow_including_descendants(document)
        .filter(|node| matches!(node.data, NodeData::ShadowRoot(_)))
    {
        for style_element in
            Node::descendants(&shadow).filter(|node| node.tag_name() == Some("style"))
        {
            parse_stylesheet(
                &style_element.text_content(),
                document_base_url,
                viewport_width,
                prefers_dark_color_scheme,
                &mut next_order,
                &mut rules,
                RuleScope::Shadow(shadow.id()),
            );
        }
        append_adopted(
            &shadow,
            viewport_width,
            prefers_dark_color_scheme,
            &mut next_order,
            &mut rules,
            RuleScope::Shadow(shadow.id()),
        );
    }
    rules
}

fn append_adopted(
    root: &NodeRef,
    viewport_width: f32,
    prefers_dark_color_scheme: bool,
    next_order: &mut u32,
    rules: &mut Vec<Rule>,
    scope: RuleScope,
) {
    for stylesheet in root.adopted_stylesheets() {
        if !stylesheet.media.trim().is_empty()
            && !media::media_matches_with_color_scheme(
                &stylesheet.media,
                viewport_width,
                prefers_dark_color_scheme,
            )
        {
            continue;
        }
        parse_stylesheet(
            &stylesheet.source,
            &stylesheet.base_url,
            viewport_width,
            prefers_dark_color_scheme,
            next_order,
            rules,
            scope,
        );
    }
}
