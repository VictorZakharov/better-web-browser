//! Immutable glyph bitmaps, never painted text runs. A context may change paint,
//! placement, spacing, baseline or direction without invalidating outline pixels.

use crate::engine::font::shaping::FontInstanceKey;
use std::collections::{HashMap, VecDeque};
use std::rc::Rc;
use swash::scale::image::Image;

const MAX_ENTRIES: usize = 2048;
const MAX_BYTES: usize = 8 * 1024 * 1024;
const MAX_IMAGE_BYTES: usize = 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) struct GlyphKey {
    pub(super) font: FontInstanceKey,
    pub(super) glyph: u16,
    pub(super) size_bits: u32,
    pub(super) stroke_bits: Option<u32>,
}

#[derive(Default)]
pub(super) struct GlyphCache {
    entries: HashMap<GlyphKey, Rc<Image>>,
    order: VecDeque<GlyphKey>,
    bytes: usize,
}

impl GlyphCache {
    pub(super) fn get(&self, key: &GlyphKey) -> Option<Rc<Image>> {
        self.entries.get(key).cloned()
    }

    pub(super) fn insert(&mut self, key: GlyphKey, image: Rc<Image>) {
        // Oversize glyphs remain usable for the current call, but cannot evict
        // every small glyph or occupy the retained cache.
        let needed = image.data.len();
        if needed > MAX_IMAGE_BYTES {
            return;
        }
        if let Some(previous) = self.entries.remove(&key) {
            self.bytes -= previous.data.len();
            self.order.retain(|previous| *previous != key);
        }
        while self.entries.len() >= MAX_ENTRIES || self.bytes + needed > MAX_BYTES {
            let Some(oldest) = self.order.pop_front() else {
                break;
            };
            if let Some(previous) = self.entries.remove(&oldest) {
                self.bytes -= previous.data.len();
            }
        }
        self.bytes += needed;
        self.entries.insert(key, image);
        self.order.push_back(key);
    }

    pub(super) fn clear(&mut self) {
        self.entries.clear();
        self.order.clear();
        self.bytes = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(glyph: u16) -> GlyphKey {
        GlyphKey {
            font: FontInstanceKey {
                blob_id: 1,
                index: 0,
                weight: 400,
                italic: false,
            },
            glyph,
            size_bits: 20f32.to_bits(),
            stroke_bits: None,
        }
    }
    fn image(bytes: usize) -> Rc<Image> {
        Rc::new(Image {
            data: vec![255; bytes],
            ..Image::default()
        })
    }

    #[test]
    fn exact_keys_separate_font_instance_size_glyph_and_stroke() {
        let mut cache = GlyphCache::default();
        let original = key(1);
        cache.insert(original, image(400));
        assert!(cache.get(&original).is_some());
        let variants = [
            GlyphKey {
                glyph: 2,
                ..original
            },
            GlyphKey {
                size_bits: 21f32.to_bits(),
                ..original
            },
            GlyphKey {
                stroke_bits: Some(1f32.to_bits()),
                ..original
            },
            GlyphKey {
                font: FontInstanceKey {
                    blob_id: 2,
                    ..original.font
                },
                ..original
            },
            GlyphKey {
                font: FontInstanceKey {
                    index: 1,
                    ..original.font
                },
                ..original
            },
            GlyphKey {
                font: FontInstanceKey {
                    weight: 700,
                    ..original.font
                },
                ..original
            },
            GlyphKey {
                font: FontInstanceKey {
                    italic: true,
                    ..original.font
                },
                ..original
            },
        ];
        for variant in variants {
            assert!(cache.get(&variant).is_none(), "{variant:?}");
        }
    }

    #[test]
    fn hits_share_only_immutable_storage_and_exported_pixels_are_independent() {
        let mut cache = GlyphCache::default();
        cache.insert(key(1), image(400));
        let first = cache.get(&key(1)).unwrap();
        let second = cache.get(&key(1)).unwrap();
        assert!(Rc::ptr_eq(&first, &second));
        let mut exported = first.data.clone();
        exported.fill(0);
        assert!(
            cache
                .get(&key(1))
                .unwrap()
                .data
                .iter()
                .all(|byte| *byte == 255)
        );
    }

    #[test]
    fn replacement_and_clear_keep_byte_accounting_exact() {
        let mut cache = GlyphCache::default();
        cache.insert(key(1), image(400));
        cache.insert(key(1), image(800));
        assert_eq!(cache.bytes, 800);
        assert_eq!(cache.order.len(), 1);
        cache.insert(key(2), image(600));
        assert_eq!(cache.bytes, 1400);
        cache.clear();
        assert_eq!(cache.bytes, 0);
        assert!(cache.order.is_empty());
        assert!(cache.entries.is_empty());
    }

    #[test]
    fn entry_and_byte_budgets_evict_oldest_without_rejecting_current_glyph() {
        let mut cache = GlyphCache::default();
        for glyph in 0..=MAX_ENTRIES as u16 {
            cache.insert(key(glyph), image(1));
        }
        assert_eq!(cache.entries.len(), MAX_ENTRIES);
        assert!(cache.get(&key(0)).is_none());
        assert!(cache.get(&key(MAX_ENTRIES as u16)).is_some());
        cache.clear();
        for glyph in 0..9 {
            cache.insert(key(glyph), image(MAX_IMAGE_BYTES));
        }
        assert_eq!(cache.bytes, MAX_BYTES);
        assert_eq!(cache.entries.len(), 8);
        assert!(cache.get(&key(0)).is_none());
        let oversized = image(MAX_IMAGE_BYTES + 1);
        cache.insert(key(99), oversized.clone());
        assert!(cache.get(&key(99)).is_none());
        assert_eq!(oversized.data.len(), MAX_IMAGE_BYTES + 1);
        assert_eq!(cache.bytes, MAX_BYTES);
    }
}
