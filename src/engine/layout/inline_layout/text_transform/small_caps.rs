//! Synthesized `font-variant-caps: small-caps`: a font without small-caps glyphs renders
//! lowercase letters as capitals at a reduced size. Line metrics keep the authored font, so
//! synthesis never changes the line box height.
//! https://drafts.csswg.org/css-fonts-4/#small-caps
use super::*;

/// The capital scale browsers use when synthesizing small caps.
const SMALL_CAPS_SCALE: f32 = 0.7;

pub(super) fn synthesize(atoms: &mut Vec<InlineAtom>) {
    let mut index = 0;
    while index < atoms.len() {
        match &mut atoms[index] {
            InlineAtom::InlineBox { children, .. } => synthesize(children),
            InlineAtom::Text {
                small_caps: true, ..
            } => {
                let runs = split(&atoms[index]);
                let count = runs.len();
                atoms.splice(index..=index, runs);
                index += count;
                continue;
            }
            _ => {}
        }
        index += 1;
    }
}

/// Splits one text atom into alternating runs of unchanged text and scaled capitals.
fn split(atom: &InlineAtom) -> Vec<InlineAtom> {
    let InlineAtom::Text {
        text,
        font,
        source_units,
        ..
    } = atom
    else {
        return vec![atom.clone()];
    };
    let mut runs: Vec<(bool, String, Vec<fragments::SourceUnit>)> = Vec::new();
    let mut unit = 0;
    for ch in text.chars() {
        let units = source_units
            .get(unit..unit + ch.len_utf16())
            .unwrap_or_default();
        unit += ch.len_utf16();
        let upper = ch.to_uppercase().collect::<String>();
        let scaled = ch.is_lowercase() && upper != ch.to_string();
        if runs.last().is_none_or(|run| run.0 != scaled) {
            runs.push((scaled, String::new(), Vec::new()));
        }
        let run = runs.last_mut().unwrap();
        let rendered = if scaled { upper } else { ch.to_string() };
        let source = units.first().copied();
        for mapped in rendered.chars() {
            run.1.push(mapped);
            if let Some(source) = source {
                run.2
                    .extend(std::iter::repeat_n(source, mapped.len_utf16()));
            }
        }
    }
    runs.into_iter()
        .map(|(scaled, run_text, run_units)| {
            let mut run = atom.clone();
            if let InlineAtom::Text {
                text,
                font: run_font,
                source_units,
                small_caps,
                ..
            } = &mut run
            {
                *text = run_text;
                *source_units = run_units;
                *small_caps = false;
                if scaled {
                    run_font.size = font.size * SMALL_CAPS_SCALE;
                }
            }
            run
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lowercase_letters_become_scaled_capitals_in_separate_runs() {
        let mut style = ComputedStyle::initial();
        style.font_size = 10.0;
        style.font_small_caps = true;
        let mut atoms = vec![super::super::tests::text_atom("Ab1", &style)];
        synthesize(&mut atoms);
        let runs = atoms
            .iter()
            .map(|atom| match atom {
                InlineAtom::Text { text, font, .. } => (text.clone(), font.size),
                _ => unreachable!(),
            })
            .collect::<Vec<_>>();
        assert_eq!(
            runs,
            [("A".into(), 10.0), ("B".into(), 7.0), ("1".into(), 10.0)]
        );
    }
}
