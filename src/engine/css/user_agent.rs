//! HTML user-agent defaults and rendering-state rules.

use super::*;

pub(crate) fn user_agent_display(tag: &str) -> Display {
    match tag {
        "html" | "body" | "address" | "article" | "aside" | "blockquote" | "center" | "details"
        | "dialog" | "div" | "dl" | "fieldset" | "figcaption" | "figure" | "footer" | "form"
        | "header" | "hgroup" | "hr" | "main" | "nav" | "ol" | "p" | "pre" | "section"
        | "summary" | "ul" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => Display::Block,
        "table" => Display::Table,
        "tr" => Display::TableRow,
        "td" | "th" => Display::TableCell,
        "img" | "input" | "button" | "select" | "textarea" | "svg" => Display::InlineBlock,
        "head" | "base" | "datalist" | "link" | "meta" | "title" | "style" | "script"
        | "template" | "rp" => Display::None,
        _ => Display::Inline,
    }
}

pub(crate) fn is_hidden_by_html_rendering(node: &NodeRef) -> bool {
    if node.tag_name() == Some("dialog") && node.attr("open").is_none() {
        return true;
    }
    let Some(parent) = node.parent() else {
        return false;
    };
    if parent.tag_name() != Some("details") || parent.attr("open").is_some() {
        return false;
    }
    let first_summary = parent
        .children
        .borrow()
        .iter()
        .find(|child| child.tag_name() == Some("summary"))
        .cloned();
    first_summary.is_none_or(|summary| summary.id() != node.id())
}

pub(super) fn apply_user_agent_defaults(node: &NodeRef, style: &mut ComputedStyle) {
    let Some(tag) = node.tag_name() else {
        return;
    };
    style.display = user_agent_display(tag);
    match tag {
        "body" => style.margin = uniform_edges(Length::Px(8.0)),
        "p" => {
            style.margin.top = Length::Em(1.0);
            style.margin.bottom = Length::Em(1.0);
        }
        "blockquote" => {
            style.margin.top = Length::Em(1.0);
            style.margin.right = Length::Px(40.0);
            style.margin.bottom = Length::Em(1.0);
            style.margin.left = Length::Px(40.0);
        }
        "h1" => heading_defaults(style, 2.0, 0.67),
        "h2" => heading_defaults(style, 1.5, 0.83),
        "h3" => heading_defaults(style, 1.17, 1.0),
        "h4" => heading_defaults(style, 1.0, 1.33),
        "h5" => heading_defaults(style, 0.83, 1.67),
        "h6" => heading_defaults(style, 0.67, 2.33),
        "pre" => {
            style.font_family = "Cascadia Mono".to_string();
            style.white_space = WhiteSpace::Pre;
            style.margin.top = Length::Em(1.0);
            style.margin.bottom = Length::Em(1.0);
        }
        "b" | "strong" => style.font_weight = 700,
        "i" | "em" => style.italic = true,
        "small" => style.font_size *= 0.833,
        "mark" => {
            style.color = Color::BLACK;
            style.background_color = Color::rgb(255, 255, 0);
        }
        "a" => {
            style.color = Color::rgb(0, 0, 238);
            style.text_decoration_underline = true;
        }
        "input" | "button" | "select" | "textarea" => {
            style.background_color = Color::WHITE;
            style.border_width = uniform_edges(Length::Px(2.0));
            style.border_color = Color::rgb(118, 118, 118);
        }
        "center" => style.text_align = TextAlign::Center,
        "th" => {
            style.font_weight = 700;
            style.text_align = TextAlign::Center;
        }
        "ul" | "ol" => {
            style.margin.top = Length::Em(1.0);
            style.margin.bottom = Length::Em(1.0);
            style.padding.left = Length::Px(40.0);
        }
        "hr" => {
            style.border_width.top = Length::Px(1.0);
            style.margin.top = Length::Em(0.5);
            style.margin.bottom = Length::Em(0.5);
        }
        _ => {}
    }
}

pub(super) fn heading_defaults(style: &mut ComputedStyle, scale: f32, margin: f32) {
    style.font_size *= scale;
    style.line_height = style.font_size * 1.2;
    style.font_weight = 700;
    style.margin.top = Length::Em(margin);
    style.margin.bottom = Length::Em(margin);
}
