use super::*;

impl InlineBoxMetrics {
    pub(super) fn total_width(self) -> f32 {
        self.margin.horizontal() + self.border_box_width
    }

    pub(super) fn total_height(self) -> f32 {
        self.margin.vertical() + self.border_box_height
    }
}

pub(super) fn collect_text_atoms(
    text: &str,
    style: &ComputedStyle,
    link: Option<(String, NodeId)>,
    source_node: NodeId,
    output: &mut Vec<InlineAtom>,
    pending_space: &mut bool,
) {
    if style.white_space == WhiteSpace::Pre {
        for (index, line) in text.replace("\r\n", "\n").split('\n').enumerate() {
            if index > 0 {
                output.push(InlineAtom::Break);
            }
            if !line.is_empty() {
                output.push(text_atom(
                    line.to_string(),
                    style,
                    link.clone(),
                    Some(source_node),
                ));
            }
        }
        return;
    }

    let mut word_start = None;
    let mut saw_space = *pending_space;
    for (index, character) in text.char_indices() {
        if character.is_whitespace() {
            if let Some(start) = word_start.take() {
                let mut word = text[start..index].to_string();
                if saw_space {
                    word.insert(0, ' ');
                }
                output.push(text_atom(word, style, link.clone(), Some(source_node)));
            }
            saw_space = true;
        } else if word_start.is_none() {
            word_start = Some(index);
        }
    }
    if let Some(start) = word_start {
        let mut word = text[start..].to_string();
        if saw_space {
            word.insert(0, ' ');
        }
        output.push(text_atom(word, style, link, Some(source_node)));
        *pending_space = false;
    } else {
        *pending_space = saw_space;
    }
    if text.chars().last().is_some_and(char::is_whitespace) {
        *pending_space = true;
    }
}

pub(super) fn text_atom(
    text: String,
    style: &ComputedStyle,
    link: Option<(String, NodeId)>,
    source_node: Option<NodeId>,
) -> InlineAtom {
    let (link, interaction_node) = match link {
        Some((url, node_id)) => (Some(url), Some(node_id)),
        None => (None, source_node),
    };
    InlineAtom::Text {
        text,
        font: FontSpec::from_style(style),
        color: style.color,
        link,
        node_id: interaction_node,
        line_height: style.line_height,
        no_wrap: style.white_space == WhiteSpace::NoWrap,
    }
}

pub(super) fn is_block_level(display: Display) -> bool {
    matches!(
        display,
        Display::Block
            | Display::Flex
            | Display::Grid
            | Display::Table
            | Display::TableRow
            | Display::TableCell
    )
}

pub(super) fn resolve_border_radius(radius: Length, rect: RectF, font_size: f32) -> f32 {
    let basis = rect.width.min(rect.height).max(0.0);
    radius
        .resolve(basis, font_size)
        .unwrap_or(0.0)
        .clamp(0.0, basis / 2.0)
}

pub(super) fn node_id(node: &NodeRef) -> NodeId {
    node.id()
}

pub(super) fn svg_uses_current_color(node: &NodeRef) -> bool {
    Node::descendants(node).any(|descendant| {
        ["fill", "stroke", "style"].iter().any(|attribute| {
            descendant
                .attr(attribute)
                .is_some_and(|value| value.to_ascii_lowercase().contains("currentcolor"))
        })
    })
}

pub(super) fn resolve_outer_size(
    length: Length,
    basis: f32,
    font_size: f32,
    insets: f32,
    box_sizing: BoxSizing,
) -> Option<f32> {
    length
        .resolve(basis, font_size)
        .map(|size| match box_sizing {
            BoxSizing::ContentBox => size + insets,
            BoxSizing::BorderBox => size,
        })
}

pub(super) fn resolve_content_height(
    length: Length,
    percentage_basis: Option<f32>,
    viewport: RectF,
    font_size: f32,
    insets: f32,
    box_sizing: BoxSizing,
) -> Option<f32> {
    resolve_height_value(length, percentage_basis, viewport, font_size).map(|size| match box_sizing
    {
        BoxSizing::ContentBox => size,
        BoxSizing::BorderBox => (size - insets).max(0.0),
    })
}

pub(super) fn resolve_height_value(
    length: Length,
    percentage_basis: Option<f32>,
    viewport: RectF,
    font_size: f32,
) -> Option<f32> {
    match length {
        Length::Auto => None,
        Length::Percent(value) => percentage_basis.map(|basis| basis * value / 100.0),
        Length::Px(value) => Some(value),
        Length::Em(value) => Some(value * font_size),
        Length::Rem(value) => Some(value * 16.0),
        Length::Vh(value) => Some(viewport.height * value / 100.0),
        Length::Vw(value) => Some(viewport.width * value / 100.0),
        Length::Vmin(value) => Some(viewport.width.min(viewport.height) * value / 100.0),
        Length::Vmax(value) => Some(viewport.width.max(viewport.height) * value / 100.0),
        Length::Calc {
            px,
            percent,
            em,
            rem,
            vw,
            vh,
            vmin,
            vmax,
        } if percent.abs() <= f32::EPSILON => Some(
            px + font_size * em
                + 16.0 * rem
                + viewport.width * vw / 100.0
                + viewport.height * vh / 100.0
                + viewport.width.min(viewport.height) * vmin / 100.0
                + viewport.width.max(viewport.height) * vmax / 100.0,
        ),
        Length::Calc {
            px,
            percent,
            em,
            rem,
            vw,
            vh,
            vmin,
            vmax,
        } => percentage_basis.map(|basis| {
            px + basis * percent / 100.0
                + font_size * em
                + 16.0 * rem
                + viewport.width * vw / 100.0
                + viewport.height * vh / 100.0
                + viewport.width.min(viewport.height) * vmin / 100.0
                + viewport.width.max(viewport.height) * vmax / 100.0
        }),
    }
}

pub(super) fn style_collapses_overflow(style: &ComputedStyle, viewport: RectF) -> bool {
    if !style.overflow_hidden {
        return false;
    }
    if resolve_height_value(style.max_height, None, viewport, style.font_size)
        .is_some_and(|height| height <= 0.0)
    {
        return true;
    }
    matches!(style.position, Position::Absolute | Position::Fixed)
        && style
            .width
            .resolve(viewport.width, style.font_size)
            .is_some_and(|width| width <= 1.0)
        && resolve_height_value(style.height, None, viewport, style.font_size)
            .is_some_and(|height| height <= 1.0)
}

pub(super) fn element_length(
    node: &NodeRef,
    attribute: &str,
    css: Length,
    fallback: f32,
    font_size: f32,
) -> f32 {
    css.resolve(fallback, font_size)
        .or_else(|| {
            node.attr(attribute)
                .and_then(|value| value.trim_end_matches("px").parse::<f32>().ok())
        })
        .unwrap_or(fallback)
        .max(0.0)
}

pub(super) fn resolve_svg_replaced_size(
    node: &NodeRef,
    style: &ComputedStyle,
    containing_block: InlineContainingBlock,
    intrinsic_width: f32,
    intrinsic_height: f32,
) -> (f32, f32) {
    let width = resolve_svg_replaced_length(
        node,
        "width",
        style.width,
        Some(containing_block.width),
        style.font_size,
    );
    let height = resolve_svg_replaced_length(
        node,
        "height",
        style.height,
        containing_block.height,
        style.font_size,
    );
    let (width, height) = match (width, height) {
        (Some(width), Some(height)) => (width, height),
        (Some(width), None) if intrinsic_width > 0.0 => {
            (width, width * intrinsic_height / intrinsic_width)
        }
        (None, Some(height)) if intrinsic_height > 0.0 => {
            (height * intrinsic_width / intrinsic_height, height)
        }
        (Some(width), None) => (width, intrinsic_height),
        (None, Some(height)) => (intrinsic_width, height),
        (None, None) => (intrinsic_width, intrinsic_height),
    };
    (width.max(0.0), height.max(0.0))
}

pub(super) fn resolve_svg_replaced_length(
    node: &NodeRef,
    attribute: &str,
    css: Length,
    percentage_basis: Option<f32>,
    font_size: f32,
) -> Option<f32> {
    let length = if css == Length::Auto {
        let value = node.attr(attribute)?;
        parse_length(&value).or_else(|| value.trim().parse::<f32>().ok().map(Length::Px))?
    } else {
        css
    };
    resolve_replaced_css_length(length, percentage_basis, font_size)
}

pub(super) fn resolve_replaced_length(
    node: &NodeRef,
    attribute: &str,
    css: Length,
    percentage_basis: Option<f32>,
    font_size: f32,
) -> Option<f32> {
    let css = resolve_replaced_css_length(css, percentage_basis, font_size);
    css.or_else(|| {
        node.attr(attribute)
            .and_then(|value| value.trim_end_matches("px").parse::<f32>().ok())
    })
    .filter(|value| value.is_finite())
    .map(|value| value.max(0.0))
}

fn resolve_replaced_css_length(
    length: Length,
    percentage_basis: Option<f32>,
    font_size: f32,
) -> Option<f32> {
    let resolved = match length {
        Length::Percent(_) => percentage_basis.and_then(|basis| length.resolve(basis, font_size)),
        Length::Calc { percent, .. } if percent.abs() > f32::EPSILON => {
            percentage_basis.and_then(|basis| length.resolve(basis, font_size))
        }
        Length::Auto => None,
        _ => length.resolve(0.0, font_size),
    };
    resolved
        .filter(|value| value.is_finite())
        .map(|value| value.max(0.0))
}
