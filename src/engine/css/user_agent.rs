//! HTML user-agent defaults and rendering-state rules.

use super::values::LineHeight;
use super::*;

pub(crate) fn user_agent_display(tag: &str) -> Display {
    match tag {
        "html" | "body" | "address" | "article" | "aside" | "blockquote" | "center" | "details"
        | "dialog" | "div" | "dl" | "fieldset" | "figcaption" | "figure" | "footer" | "form"
        | "dd" | "dt" | "header" | "hgroup" | "hr" | "li" | "main" | "menu" | "nav" | "ol"
        | "p" | "pre" | "section" | "summary" | "ul" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
            Display::Block
        }
        "table" => Display::Table,
        "tr" => Display::TableRow,
        "td" | "th" => Display::TableCell,
        "caption" => Display::TableCaption,
        "tbody" => Display::TableRowGroup,
        "thead" => Display::TableHeaderGroup,
        "tfoot" => Display::TableFooterGroup,
        "col" => Display::TableColumn,
        "colgroup" => Display::TableColumnGroup,
        // HTML defines slot as a box-tree-transparent insertion point. Its assigned nodes retain
        // their own outer display and participate directly in the host's formatting context.
        "slot" => Display::Contents,
        "img" | "video" | "audio" | "input" | "button" | "select" | "textarea" | "svg" => {
            Display::InlineBlock
        }
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

pub(super) fn apply_user_agent_defaults(
    node: &NodeRef,
    style: &mut ComputedStyle,
    parent: Option<&ComputedStyle>,
) {
    let Some(tag) = node.tag_name() else {
        return;
    };
    style.display = user_agent_display(tag);
    // HTML's UA rules, not an inherited CSS property: row groups start in the
    // middle and rows/cells inherit their parent's alignment below author rules.
    // https://html.spec.whatwg.org/multipage/rendering.html#tables
    if matches!(tag, "thead" | "tbody" | "tfoot")
        || (tag == "tr" && node.parent().is_some_and(|p| p.tag_name() == Some("table")))
    {
        style.vertical_align = VerticalAlign::Middle;
    } else if matches!(tag, "tr" | "td" | "th") {
        style.vertical_align = parent.map_or(VerticalAlign::Middle, |p| p.vertical_align);
    }
    if matches!(tag, "thead" | "tbody" | "tfoot" | "tr" | "td" | "th")
        && let Some(align) = node.attr("valign").and_then(|v| VerticalAlign::parse(&v))
    {
        style.vertical_align = align;
    }
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
            if node.tag_name() != Some("select") {
                style.line_height_value = LineHeight::Normal;
            }
            if node.tag_name() == Some("button") {
                // HTML's default button styling measures authored sizes at the border box.
                style.box_sizing = BoxSizing::BorderBox;
                style.text_align = TextAlign::Center;
                style.align_content = ContentAlignment::CENTER;
            }
            style.background_color = Color::WHITE;
            style.border_width = uniform_edges(Length::Px(2.0));
            style.border_colors = [Some(Color::rgb(118, 118, 118)); 4];
        }
        "table" => {
            style.box_sizing = BoxSizing::BorderBox;
            // HTML's suggested UA sheet uses 2px in the separated model.
            // https://html.spec.whatwg.org/multipage/rendering.html#tables-2
            style.border_spacing = [Length::Px(2.0); 2];
        }
        "center" => style.text_align = TextAlign::Center,
        "th" => {
            style.padding = uniform_edges(Length::Px(1.0));
            style.font_weight = 700;
            style.text_align = TextAlign::Center;
        }
        "td" => style.padding = uniform_edges(Length::Px(1.0)),
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
    style.font_weight = 700;
    style.margin.top = Length::Em(margin);
    style.margin.bottom = Length::Em(margin);
}
