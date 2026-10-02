//! Named keyframe definitions, separate from selector rules and declaration cascade.
//! Duplicate offsets merge declarations in source order; !important is ignored.
//! https://drafts.csswg.org/css-animations-1/#keyframes
use super::*;
use crate::engine::css::values::animations::AnimationName;
mod collection;
pub(in crate::engine::css) use collection::collect_with_layers;

#[cfg(test)]
mod tests;

pub(crate) const MAX_KEYFRAME_BLOCKS: usize = 256;
pub(crate) const MAX_KEYFRAME_DEFINITIONS: usize = 512;

#[derive(Clone, Debug)]
pub(crate) struct KeyframeBlock {
    pub(crate) offsets: Vec<f32>,
    pub(crate) declarations: Vec<(String, String)>,
}

#[derive(Clone, Debug)]
pub(crate) struct KeyframeDefinition {
    pub(crate) name: String,
    pub(crate) blocks: Vec<KeyframeBlock>,
    pub(in crate::engine::css) scope: RuleScope,
    pub(in crate::engine::css) layer: LayerPath,
    pub(in crate::engine::css) layer_rank: u32,
}

pub(crate) fn name(value: &str) -> Option<String> {
    match AnimationName::parse(value)? {
        AnimationName::Named(name) | AnimationName::Quoted(name) => Some(name),
        AnimationName::None => None,
    }
}

pub(crate) fn name_text(value: String) -> String {
    AnimationName::Quoted(value).css_text()
}

pub(crate) fn offsets(value: &str) -> Option<Vec<f32>> {
    let mut result = Vec::new();
    for part in split_css_top_level(value, ',') {
        if result.len() == MAX_KEYFRAME_BLOCKS {
            return None;
        }
        let mut input = ParserInput::new(part.trim());
        let mut parser = Parser::new(&mut input);
        let offset = match parser.next().ok()?.clone() {
            Token::Ident(value) if value.eq_ignore_ascii_case("from") => 0.0,
            Token::Ident(value) if value.eq_ignore_ascii_case("to") => 1.0,
            Token::Percentage { unit_value, .. }
                if unit_value.is_finite() && (0.0..=1.0).contains(&unit_value) =>
            {
                unit_value
            }
            _ => return None,
        };
        if !parser.is_exhausted() {
            return None;
        }
        if !result.contains(&offset) {
            result.push(offset);
        }
    }
    (!result.is_empty()).then_some(result)
}

pub(crate) fn block(source: &str) -> Option<KeyframeBlock> {
    let source = strip_comments(source);
    let open = find_css_delimiter(&source, 0, '{')?;
    let close = find_matching_brace(&source, open)?;
    if !source[close + 1..].trim().is_empty() {
        return None;
    }
    let offsets = offsets(&source[..open])?;
    let declarations = parse_declarations(&source[open + 1..close])
        .into_iter()
        .filter(|declaration| !declaration.important)
        .map(|declaration| (declaration.name, declaration.value))
        .collect();
    Some(KeyframeBlock {
        offsets,
        declarations,
    })
}

pub(crate) fn blocks(source: &str) -> Vec<KeyframeBlock> {
    let source = strip_comments(source);
    let mut cursor = 0;
    let mut output = Vec::new();
    while output.len() < MAX_KEYFRAME_BLOCKS {
        cursor = skip_css_whitespace(&source, cursor);
        let Some(open) = find_css_delimiter(&source, cursor, '{') else {
            break;
        };
        let Some(close) = find_matching_brace(&source, open) else {
            break;
        };
        if let Some(block) = block(&source[cursor..close + 1]) {
            output.push(block);
        }
        cursor = close + 1;
    }
    output
}

#[cfg(test)]
pub(in crate::engine::css) fn collect(
    css: &str,
    environment: MediaEnvironment,
    scope: RuleScope,
) -> Vec<KeyframeDefinition> {
    collect_with_layers(css, environment, scope).definitions
}

impl KeyframeDefinition {
    pub(crate) fn merged_blocks(&self) -> Vec<KeyframeBlock> {
        let mut merged = Vec::<KeyframeBlock>::new();
        for block in &self.blocks {
            for &offset in &block.offsets {
                let index = merged.iter().position(|block| block.offsets[0] == offset);
                let target = if let Some(index) = index {
                    &mut merged[index]
                } else {
                    if merged.len() == MAX_KEYFRAME_BLOCKS {
                        continue;
                    }
                    merged.push(KeyframeBlock {
                        offsets: vec![offset],
                        declarations: Vec::new(),
                    });
                    merged.last_mut().unwrap()
                };
                for (name, value) in &block.declarations {
                    if let Some(prior) = target
                        .declarations
                        .iter_mut()
                        .find(|(prior, _)| prior == name)
                    {
                        prior.1.clone_from(value);
                    } else {
                        target.declarations.push((name.clone(), value.clone()));
                    }
                }
            }
        }
        merged.sort_by(|a, b| a.offsets[0].total_cmp(&b.offsets[0]));
        merged
    }
}
