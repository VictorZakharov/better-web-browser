//! Higher-level CSS Fonts 4 feature controls. Unset groups retain the font's
//! defaults; property features precede explicit font-feature-settings.
//! https://drafts.csswg.org/css-fonts-4/#font-variant-ligatures-prop
//! https://drafts.csswg.org/css-fonts-4/#font-variant-numeric-prop
use cssparser::{Parser, ParserInput};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct FontLigatures(u16);
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct FontNumeric(u8);
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct FontVariants(u16);

const LIGATURE_NAMES: [[&str; 2]; 4] = [
    ["common-ligatures", "no-common-ligatures"],
    ["discretionary-ligatures", "no-discretionary-ligatures"],
    ["historical-ligatures", "no-historical-ligatures"],
    ["contextual", "no-contextual"],
];
const NUMERIC_NAMES: [[&str; 2]; 3] = [
    ["lining-nums", "oldstyle-nums"],
    ["proportional-nums", "tabular-nums"],
    ["diagonal-fractions", "stacked-fractions"],
];

fn tokens(value: &str) -> Option<Vec<String>> {
    if value.len() > 4096 {
        return None;
    }
    let mut input = ParserInput::new(value);
    let mut parser = Parser::new(&mut input);
    let mut names = Vec::new();
    while !parser.is_exhausted() {
        if names.len() == 16 {
            return None;
        }
        names.push(parser.expect_ident_cloned().ok()?.to_ascii_lowercase());
    }
    (!names.is_empty()).then_some(names)
}

fn set_group(bits: &mut u8, group: usize, option: usize) -> bool {
    let shift = group * 2;
    if *bits & (3 << shift) != 0 {
        return false;
    }
    *bits |= ((option + 1) as u8) << shift;
    true
}

fn group(bits: u8, index: usize) -> u8 {
    (bits >> (index * 2)) & 3
}

fn grouped_text(bits: u8, names: &[[&'static str; 2]]) -> Vec<&'static str> {
    names
        .iter()
        .enumerate()
        .filter_map(|(index, names)| match group(bits, index) {
            1 => Some(names[0]),
            2 => Some(names[1]),
            _ => None,
        })
        .collect()
}

impl FontLigatures {
    pub(crate) fn parse(value: &str) -> Option<Self> {
        let names = tokens(value)?;
        match names.as_slice() {
            [name] if name == "normal" => return Some(Self::default()),
            [name] if name == "none" => return Some(Self(0x1aa)),
            _ => {}
        }
        let mut result = Self::default();
        for name in names {
            if !result.add(&name) {
                return None;
            }
        }
        Some(result)
    }
    fn add(&mut self, name: &str) -> bool {
        for (index, options) in LIGATURE_NAMES.iter().enumerate() {
            if let Some(option) = options.iter().position(|value| *value == name) {
                let mut bits = self.0 as u8;
                if !set_group(&mut bits, index, option) {
                    return false;
                }
                self.0 = u16::from(bits);
                return true;
            }
        }
        false
    }
    pub(crate) fn css_text(self) -> String {
        match self.0 {
            0 => "normal".into(),
            0x1aa => "none".into(),
            _ => grouped_text(self.0 as u8, &LIGATURE_NAMES).join(" "),
        }
    }
}

impl FontNumeric {
    pub(crate) fn parse(value: &str) -> Option<Self> {
        let names = tokens(value)?;
        if names == ["normal"] {
            return Some(Self::default());
        }
        let mut result = Self::default();
        for name in names {
            if !result.add(&name) {
                return None;
            }
        }
        Some(result)
    }
    fn add(&mut self, name: &str) -> bool {
        for (index, options) in NUMERIC_NAMES.iter().enumerate() {
            if let Some(option) = options.iter().position(|value| *value == name) {
                return set_group(&mut self.0, index, option);
            }
        }
        let bit = match name {
            "ordinal" => 0x40,
            "slashed-zero" => 0x80,
            _ => return false,
        };
        if self.0 & bit != 0 {
            return false;
        }
        self.0 |= bit;
        true
    }
    pub(crate) fn css_text(self) -> String {
        if self.0 == 0 {
            return "normal".into();
        }
        let mut values = grouped_text(self.0, &NUMERIC_NAMES);
        if self.0 & 0x40 != 0 {
            values.push("ordinal");
        }
        if self.0 & 0x80 != 0 {
            values.push("slashed-zero");
        }
        values.join(" ")
    }
}

impl FontVariants {
    pub(crate) fn css_text(ligatures: FontLigatures, numeric: FontNumeric) -> String {
        let ligatures = ligatures.css_text();
        let numeric = numeric.css_text();
        match (ligatures.as_str(), numeric.as_str()) {
            ("normal", "normal") => "normal".into(),
            (_, "normal") => ligatures,
            ("normal", _) => numeric,
            ("none", _) => String::new(),
            _ => format!("{ligatures} {numeric}"),
        }
    }
    pub(crate) fn new(ligatures: FontLigatures, numeric: FontNumeric) -> Self {
        Self((ligatures.0 & 0xff) | u16::from(numeric.0) << 8)
    }
    pub(crate) fn parse_shorthand(value: &str) -> Option<(FontLigatures, FontNumeric)> {
        let names = tokens(value)?;
        if names == ["normal"] {
            return Some((FontLigatures::default(), FontNumeric::default()));
        }
        if names == ["none"] {
            return Some((FontLigatures(0x1aa), FontNumeric::default()));
        }
        let mut ligatures = FontLigatures::default();
        let mut numeric = FontNumeric::default();
        for name in names {
            if !ligatures.add(&name) && !numeric.add(&name) {
                return None;
            }
        }
        Some((ligatures, numeric))
    }
    pub(crate) fn bits(self) -> u16 {
        self.0
    }
    pub(crate) fn from_bits(bits: u16) -> Option<Self> {
        (0..7)
            .all(|index| ((bits >> (index * 2)) & 3) != 3)
            .then_some(Self(bits))
    }
    pub(crate) fn settings(self) -> Vec<([u8; 4], u32)> {
        let mut values = Vec::new();
        for (index, tags) in [
            [*b"liga", *b"clig"],
            [*b"dlig", *b"dlig"],
            [*b"hlig", *b"hlig"],
            [*b"calt", *b"calt"],
        ]
        .iter()
        .enumerate()
        {
            match group(self.0 as u8, index) {
                1 | 2 => {
                    let value = u32::from(group(self.0 as u8, index) == 1);
                    values.push((tags[0], value));
                    if tags[0] != tags[1] {
                        values.push((tags[1], value));
                    }
                }
                _ => {}
            }
        }
        let numeric = (self.0 >> 8) as u8;
        for (index, tags) in [
            [*b"lnum", *b"onum"],
            [*b"pnum", *b"tnum"],
            [*b"frac", *b"afrc"],
        ]
        .iter()
        .enumerate()
        {
            match group(numeric, index) {
                1 => {
                    values.push((tags[0], 1));
                    values.push((tags[1], 0));
                }
                2 => {
                    values.push((tags[0], 0));
                    values.push((tags[1], 1));
                }
                _ => {}
            }
        }
        if numeric & 0x40 != 0 {
            values.push((*b"ordn", 1));
        }
        if numeric & 0x80 != 0 {
            values.push((*b"zero", 1));
        }
        values
    }
}

#[cfg(test)]
mod tests;
