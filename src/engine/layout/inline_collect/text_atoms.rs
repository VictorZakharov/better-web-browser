//! CSS whitespace processing with a DOM UTF-16 source map.
use super::*;
use fragments::{PendingSpace, SourceUnit};

pub(super) fn collect_text_atoms(
    text: &str,
    style: &ComputedStyle,
    link: Option<(String, NodeId)>,
    source_node: Option<NodeId>,
    output: &mut Vec<InlineAtom>,
    pending: &mut PendingSpace,
) {
    let mut word = String::new();
    let mut units = Vec::new();
    let mut offset = 0;
    let mut chars = text.chars().peekable();
    while let Some(mut ch) = chars.next() {
        let start = offset;
        offset += ch.len_utf16() as u32;
        if ch == '\r' {
            if chars.peek() == Some(&'\n') {
                chars.next();
                offset += 1;
            }
            ch = '\n';
        }
        let unit = SourceUnit {
            node: source_node,
            start,
            end: offset,
        };
        if style.white_space.preserves_spaces() {
            if ch == '\n' {
                emit(&mut word, &mut units, style, &link, source_node, output);
                output.push(InlineAtom::Break);
            } else {
                // Preserved spaces belong to the preceding line, unlike collapsed spaces.
                // CSS Text §3/§4: pre-wrap permits a break after each space sequence.
                if style.white_space.wraps()
                    && !matches!(ch, ' ' | '\t')
                    && word.ends_with([' ', '\t'])
                {
                    emit(&mut word, &mut units, style, &link, source_node, output);
                }
                word.push(ch);
                units.extend(std::iter::repeat_n(unit, ch.len_utf16()));
            }
        } else if matches!(ch, ' ' | '\t' | '\n' | '\u{000c}') {
            emit(&mut word, &mut units, style, &link, source_node, output);
            if let Some(space) = pending.as_mut() {
                if space.node == unit.node {
                    space.end = unit.end;
                }
            } else {
                *pending = Some(unit);
            }
        } else {
            if let Some(space) = pending.take() {
                word.push(' ');
                units.push(space);
            }
            word.push(ch);
            units.extend(std::iter::repeat_n(unit, ch.len_utf16()));
        }
    }
    emit(&mut word, &mut units, style, &link, source_node, output);
}

fn emit(
    text: &mut String,
    units: &mut Vec<SourceUnit>,
    style: &ComputedStyle,
    link: &Option<(String, NodeId)>,
    source_node: Option<NodeId>,
    output: &mut Vec<InlineAtom>,
) {
    if text.is_empty() {
        return;
    }
    let mut atom = text_atom(std::mem::take(text), style, link.clone(), source_node);
    if let InlineAtom::Text { source_units, .. } = &mut atom {
        *source_units = std::mem::take(units);
    }
    output.push(atom);
}

pub(super) fn pending_space_atom(
    pending: &mut PendingSpace,
    style: &ComputedStyle,
    link: Option<(String, NodeId)>,
) -> InlineAtom {
    let mut atom = text_atom(" ".into(), style, link, None);
    if let InlineAtom::Text { source_units, .. } = &mut atom {
        source_units.extend(pending.take());
    }
    atom
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
        source_node,
        source_units: Vec::new(),
        visible: style.visibility,
        preserve_space: style.white_space.preserves_spaces(),
        line_height: style.line_height,
        no_wrap: !style.white_space.wraps(),
    }
}
