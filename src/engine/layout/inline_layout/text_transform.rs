//! CSS Text 3 visual case conversion over a complete inline formatting run.
//!
//! DOM strings remain unchanged. Every transformed UTF-16 code unit retains
//! the original source span so Range and selection geometry still address the
//! source text even when a case mapping expands to multiple characters.

use super::*;
use std::borrow::Cow;
use std::collections::HashSet;
use unicode_segmentation::UnicodeSegmentation;

pub(super) fn apply(atoms: &[InlineAtom]) -> Cow<'_, [InlineAtom]> {
    if !contains_transformation(atoms) {
        return Cow::Borrowed(atoms);
    }
    let mut source = String::new();
    append_source(atoms, &mut source);
    let word_starts = source
        .unicode_word_indices()
        .filter_map(|(start, word)| {
            word.char_indices()
                .find(|(_, ch)| ch.is_alphabetic())
                .map(|(offset, _)| start + offset)
        })
        .collect::<HashSet<_>>();
    let mut transformed = atoms.to_vec();
    transform_atoms(&mut transformed, &word_starts, &mut 0);
    Cow::Owned(transformed)
}

fn contains_transformation(atoms: &[InlineAtom]) -> bool {
    atoms.iter().any(|atom| match atom {
        InlineAtom::Text { text_transform, .. } => *text_transform != TextTransform::None,
        InlineAtom::InlineBox { children, .. } => contains_transformation(children),
        _ => false,
    })
}

fn append_source(atoms: &[InlineAtom], source: &mut String) {
    for atom in atoms {
        match atom {
            InlineAtom::Text { text, .. } => source.push_str(text),
            InlineAtom::InlineBox { children, .. } => append_source(children, source),
            _ => source.push('\u{fffc}'),
        }
    }
}

fn transform_atoms(atoms: &mut [InlineAtom], starts: &HashSet<usize>, cursor: &mut usize) {
    for atom in atoms {
        match atom {
            InlineAtom::Text {
                text,
                source_units,
                text_transform,
                ..
            } => {
                let length = text.len();
                if *text_transform != TextTransform::None {
                    let mut rendered = String::with_capacity(length);
                    let mut mapped_units = Vec::with_capacity(source_units.len());
                    let mut unit_offset = 0;
                    for (byte_offset, ch) in text.char_indices() {
                        let source = source_units.get(unit_offset).copied();
                        unit_offset += ch.len_utf16();
                        let convert = match text_transform {
                            TextTransform::Uppercase => true,
                            TextTransform::Capitalize => starts.contains(&(*cursor + byte_offset)),
                            _ => false,
                        };
                        let transformed = if convert {
                            ch.to_uppercase().collect::<String>()
                        } else if *text_transform == TextTransform::Lowercase {
                            ch.to_lowercase().collect::<String>()
                        } else {
                            ch.to_string()
                        };
                        for mapped in transformed.chars() {
                            rendered.push(mapped);
                            if let Some(source) = source {
                                mapped_units
                                    .extend(std::iter::repeat_n(source, mapped.len_utf16()));
                            }
                        }
                    }
                    *text = rendered;
                    *source_units = mapped_units;
                }
                *cursor += length;
            }
            InlineAtom::InlineBox { children, .. } => transform_atoms(children, starts, cursor),
            _ => *cursor += '\u{fffc}'.len_utf8(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fragments::SourceUnit;

    fn text_atom(text: &str, style: &ComputedStyle) -> InlineAtom {
        InlineAtom::Text {
            text: text.into(),
            font: FontSpec::from_style(style),
            color: style.color,
            link: None,
            node_id: None,
            source_node: None,
            source_units: Vec::new(),
            text_transform: style.text_transform,
            visible: true,
            preserve_space: false,
            line_height: style.line_height,
            no_wrap: false,
        }
    }

    #[test]
    fn capitalization_crosses_text_atoms_and_inline_boxes() {
        let mut style = ComputedStyle::initial();
        style.text_transform = TextTransform::Capitalize;
        let atoms = vec![
            text_atom("he", &style),
            InlineAtom::InlineBox {
                children: vec![text_atom("llo world", &style)],
                style: Box::new(style),
                node_id: None,
            },
        ];
        let result = apply(&atoms);
        assert!(matches!(&result[0], InlineAtom::Text { text, .. } if text == "He"));
        assert!(matches!(&result[1], InlineAtom::InlineBox { children, .. }
            if matches!(&children[0], InlineAtom::Text { text, .. } if text == "llo World")));
    }

    #[test]
    fn expanded_case_mappings_retain_each_original_utf16_span() {
        for (kind, expected, offsets) in [
            (TextTransform::Uppercase, "SSİ😀", vec![0, 0, 1, 2, 2]),
            (TextTransform::Lowercase, "ßi\u{307}😀", vec![0, 1, 1, 2, 2]),
        ] {
            let mut style = ComputedStyle::initial();
            style.text_transform = kind;
            let mut atom = text_atom("ßİ😀", &style);
            if let InlineAtom::Text { source_units, .. } = &mut atom {
                *source_units = [(0, 1), (1, 2), (2, 4), (2, 4)]
                    .into_iter()
                    .map(|(start, end)| SourceUnit {
                        node: None,
                        start,
                        end,
                    })
                    .collect();
            }
            let atoms = [atom];
            let transformed = apply(&atoms);
            let InlineAtom::Text {
                text, source_units, ..
            } = &transformed[0]
            else {
                panic!("expected a transformed text atom");
            };
            assert_eq!(text, expected);
            assert_eq!(source_units.len(), expected.encode_utf16().count());
            assert_eq!(
                source_units
                    .iter()
                    .map(|unit| unit.start)
                    .collect::<Vec<_>>(),
                offsets
            );
            assert_eq!(source_units.last().unwrap().end, 4);
        }
    }

    #[test]
    fn no_case_conversion_keeps_the_original_atom_slice() {
        let style = ComputedStyle::initial();
        let atoms = [text_atom("unchanged", &style)];
        assert!(matches!(apply(&atoms), Cow::Borrowed(_)));
    }
}
