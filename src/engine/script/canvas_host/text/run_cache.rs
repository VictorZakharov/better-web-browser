//! Bounded reuse of immutable shaped/rasterized runs. Paint, baseline, placement,
//! transform, clipping and compositing are deliberately not cached here.
use crate::engine::FontSpec;
use crate::engine::font::shaping::ShapeOptions;
use crate::engine::script::JsValue;
use std::collections::VecDeque;

const MAX_ENTRIES: usize = 64;
const MAX_BYTES: usize = 8 * 1024 * 1024;
const MAX_ENTRY_BYTES: usize = 512 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct RunKey {
    text: String,
    family: String,
    size: u32,
    weight: u16,
    italic: bool,
    letter_spacing: u32,
    word_spacing: u32,
    stroke: Option<u32>,
    rtl: Option<bool>,
    language: Option<harfrust::Language>,
    kerning: bool,
    features: crate::engine::css::FontFeatures,
    variants: crate::engine::css::FontVariants,
}

impl RunKey {
    pub(super) fn new(
        text: &str,
        font: &FontSpec,
        stroke: Option<f32>,
        options: &ShapeOptions,
    ) -> Self {
        Self {
            text: text.into(),
            family: font.family.clone(),
            size: font.size.to_bits(),
            weight: font.weight,
            italic: font.italic,
            letter_spacing: font.letter_spacing.to_bits(),
            word_spacing: font.word_spacing.to_bits(),
            stroke: stroke.map(f32::to_bits),
            rtl: options.rtl,
            language: options.language.clone(),
            kerning: options.kerning,
            features: font.features.clone(),
            variants: font.variants,
        }
    }
    fn bytes(&self) -> usize {
        std::mem::size_of::<Self>()
            + self.text.len()
            + self.family.len()
            + std::mem::size_of_val(self.features.settings())
            + self
                .language
                .as_ref()
                .map_or(0, |language| language.as_str().len())
    }
}

struct Entry {
    key: RunKey,
    value: JsValue,
    bytes: usize,
}

#[derive(Default)]
pub(super) struct RunCache {
    entries: VecDeque<Entry>,
    bytes: usize,
}

impl RunCache {
    pub(super) fn get(&mut self, key: &RunKey) -> Option<JsValue> {
        let index = self.entries.iter().position(|entry| &entry.key == key)?;
        let entry = self.entries.remove(index)?;
        // JsValue owns every nested array and byte vector. The exported value
        // cannot mutate the retained run, including after ArrayBuffer transfer.
        let value = entry.value.clone();
        self.entries.push_back(entry);
        Some(value)
    }

    pub(super) fn insert(&mut self, key: RunKey, value: &JsValue) {
        let Some(bytes) = retained_bytes(value, 0).and_then(|bytes| bytes.checked_add(key.bytes()))
        else {
            return;
        };
        if bytes > MAX_ENTRY_BYTES {
            return;
        }
        if let Some(index) = self.entries.iter().position(|entry| entry.key == key) {
            let previous = self.entries.remove(index).expect("located entry");
            self.bytes -= previous.bytes;
        }
        while self.entries.len() >= MAX_ENTRIES || self.bytes + bytes > MAX_BYTES {
            let Some(previous) = self.entries.pop_front() else {
                break;
            };
            self.bytes -= previous.bytes;
        }
        self.entries.push_back(Entry {
            key,
            value: value.clone(),
            bytes,
        });
        self.bytes += bytes;
    }

    pub(super) fn clear(&mut self) {
        self.entries.clear();
        self.bytes = 0;
    }
}

fn retained_bytes(value: &JsValue, depth: u8) -> Option<usize> {
    if depth > 8 {
        return None;
    }
    let overhead = std::mem::size_of::<JsValue>();
    match value {
        JsValue::Array(parts) => parts.iter().try_fold(overhead, |bytes, part| {
            bytes.checked_add(retained_bytes(part, depth + 1)?)
        }),
        JsValue::Bytes(bytes) => overhead.checked_add(bytes.len()),
        // The trusted text provider emits only arrays, owned bytes and scalar
        // numbers/flags. Do not retain new object/string variants by accident.
        JsValue::Number(_) | JsValue::Boolean(_) | JsValue::Null => Some(overhead),
        _ => None,
    }
}

#[cfg(test)]
mod tests;
