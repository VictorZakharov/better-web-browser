//! Bounded one-shot resource unions. Snapshot replacement cannot discard resource deltas.

use super::RendererPresentation;
use crate::limits::{
    MAX_GLYPH_RASTERS, MAX_PRESENTED_GLYPH_BYTES, MAX_PRESENTED_IMAGES,
    MAX_RENDERER_PRESENTATION_BYTES,
};
use std::collections::HashSet;

pub(super) fn merged_bytes(
    previous: &RendererPresentation,
    next: &RendererPresentation,
) -> Option<usize> {
    let retired = next
        .retired_image_keys
        .iter()
        .map(String::as_str)
        .collect::<HashSet<_>>();
    let updated = next
        .images
        .iter()
        .map(|image| image.url.as_str())
        .collect::<HashSet<_>>();
    let mut images = HashSet::new();
    let mut bytes = 20_usize; // Counts and glyph epoch in the resource section.
    for image in next.images.iter().rev().chain(previous.images.iter().rev()) {
        if !retired.contains(image.url.as_str()) && images.insert(image.url.as_str()) {
            bytes = bytes
                .saturating_add(image.url.len())
                .saturating_add(16)
                .saturating_add(image.image.bgra.len());
        }
    }
    let mut retirements = HashSet::new();
    for key in previous
        .retired_image_keys
        .iter()
        .chain(&next.retired_image_keys)
    {
        if !updated.contains(key.as_str()) && retirements.insert(key.as_str()) {
            bytes = bytes.saturating_add(key.len()).saturating_add(4);
        }
    }
    let mut glyphs = HashSet::new();
    let mut glyph_bytes = 0_usize;
    let previous_glyphs = previous
        .glyphs
        .iter()
        .filter(|_| previous.glyph_epoch == next.glyph_epoch);
    for glyph in next.glyphs.iter().rev().chain(previous_glyphs.rev()) {
        if glyphs.insert(glyph.id) {
            glyph_bytes = glyph_bytes.saturating_add(glyph.image.bgra.len());
            bytes = bytes
                .saturating_add(17)
                .saturating_add(glyph.image.bgra.len());
        }
    }
    (images.len() <= MAX_PRESENTED_IMAGES
        && retirements.len() <= MAX_PRESENTED_IMAGES
        && glyphs.len() <= MAX_GLYPH_RASTERS
        && glyph_bytes <= MAX_PRESENTED_GLYPH_BYTES
        && bytes <= MAX_RENDERER_PRESENTATION_BYTES)
        .then_some(bytes)
}

pub(super) fn merge(previous: &mut RendererPresentation, next: &mut RendererPresentation) {
    // Later updates cancel old retirements; later retirements cancel old updates.
    let updated = next
        .images
        .iter()
        .map(|image| image.url.as_str())
        .collect::<HashSet<_>>();
    previous
        .retired_image_keys
        .retain(|key| !updated.contains(key.as_str()));
    let retired = next
        .retired_image_keys
        .iter()
        .map(String::as_str)
        .collect::<HashSet<_>>();
    previous
        .images
        .retain(|image| !retired.contains(image.url.as_str()));
    previous
        .retired_image_keys
        .append(&mut next.retired_image_keys);
    previous.retired_image_keys.sort_unstable();
    previous.retired_image_keys.dedup();
    next.retired_image_keys = std::mem::take(&mut previous.retired_image_keys);
    previous.images.append(&mut next.images);
    // Retain the last bitmap for each identity, including repeated Canvas updates. Reverse
    // twice keeps the surviving updates in their original delivery order without cloning BGRA.
    previous.images.reverse();
    let mut images = HashSet::new();
    previous
        .images
        .retain(|image| images.insert(image.url.clone()));
    previous.images.reverse();
    next.images = std::mem::take(&mut previous.images);
    if previous.glyph_epoch == next.glyph_epoch {
        previous.glyphs.append(&mut next.glyphs);
        next.glyphs = std::mem::take(&mut previous.glyphs);
    }
    next.glyphs.reverse();
    let mut glyphs = HashSet::new();
    next.glyphs.retain(|glyph| glyphs.insert(glyph.id));
    next.glyphs.reverse();
}

pub(super) fn resource_bytes(value: &RendererPresentation) -> usize {
    let images = value.images.iter().fold(20_usize, |bytes, image| {
        bytes
            .saturating_add(image.url.len())
            .saturating_add(16)
            .saturating_add(image.image.bgra.len())
    });
    let retired = value.retired_image_keys.iter().fold(images, |bytes, key| {
        bytes.saturating_add(key.len()).saturating_add(4)
    });
    value.glyphs.iter().fold(retired, |bytes, glyph| {
        bytes
            .saturating_add(17)
            .saturating_add(glyph.image.bgra.len())
    })
}

#[cfg(test)]
mod tests;
