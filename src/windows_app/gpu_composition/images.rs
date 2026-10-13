use super::*;
use std::collections::HashMap;

const MAX_IMAGE_BYTES: usize = 64 * 1024 * 1024;
struct Entry {
    owner: Arc<[u8]>,
    width: u32,
    height: u32,
    bitmap: ID2D1Bitmap1,
    used: u64,
}
pub(super) struct Bitmaps {
    entries: HashMap<(String, Option<[u8; 4]>), Entry>,
    epoch: u64,
    bytes: usize,
    limit: usize,
    high_water: usize,
}

impl Bitmaps {
    pub(super) fn new(budget: better_web_browser::renderer_budget::RendererBudget) -> Self {
        Self {
            entries: HashMap::new(),
            epoch: 0,
            bytes: 0,
            limit: budget.presentation_bitmap_bytes(),
            high_water: 0,
        }
    }
    pub(super) fn report(&self) -> serde_json::Value {
        serde_json::json!({"pixel_bytes":self.bytes,"pixel_byte_limit":self.limit,
            "pixel_bytes_high_water":self.high_water,"entries":self.entries.len(),
            "scope":"cached bitmap pixel payloads; excludes driver allocation overhead"})
    }
    pub(super) fn generated(
        &mut self,
        device: &Device,
        key: &str,
        generate: impl FnOnce() -> Result<DecodedImage, String>,
    ) -> Result<ID2D1Bitmap1, String> {
        let cache_key = (key.to_owned(), None);
        if let Some(entry) = self.entries.get_mut(&cache_key) {
            entry.used = self.epoch;
            return Ok(entry.bitmap.clone());
        }
        self.get(device, key, &generate()?, None)
    }
    pub(super) fn next_frame(&mut self) -> Result<(), String> {
        self.epoch = self
            .epoch
            .checked_add(1)
            .ok_or("GPU bitmap cache epoch exhausted")?;
        Ok(())
    }

    pub(super) fn get(
        &mut self,
        device: &Device,
        key: &str,
        image: &DecodedImage,
        tint: Option<[u8; 4]>,
    ) -> Result<ID2D1Bitmap1, String> {
        let cache_key = (key.to_owned(), tint);
        if let Some(entry) = self.entries.get_mut(&cache_key)
            && Arc::ptr_eq(&entry.owner, &image.bgra)
            && (entry.width, entry.height) == (image.width, image.height)
        {
            entry.used = self.epoch;
            return Ok(entry.bitmap.clone());
        }
        if let Some(entry) = self.entries.remove(&cache_key) {
            self.bytes -= entry.owner.len();
        }
        let bytes = image.bgra.len();
        if bytes > MAX_IMAGE_BYTES {
            return Err("GPU bitmap exceeds the 64 MiB cache limit".into());
        }
        while self.bytes.saturating_add(bytes) > self.limit || self.entries.len() >= 4096 {
            let Some(key) = self
                .entries
                .iter()
                .filter(|(_, entry)| entry.used != self.epoch)
                .min_by_key(|(_, entry)| entry.used)
                .map(|(key, _)| key.clone())
            else {
                return Err(format!(
                    "GPU page working set exceeds the {} byte bitmap cache limit: {} resident bytes in {} entries, requested {}x{} / {bytes} bytes",
                    self.limit,
                    self.bytes,
                    self.entries.len(),
                    image.width,
                    image.height,
                ));
            };
            if let Some(entry) = self.entries.remove(&key) {
                self.bytes -= entry.owner.len();
            }
        }
        let tinted = tint.map(|color| {
            let mut pixels = Vec::with_capacity(bytes);
            for pixel in image.bgra.chunks_exact(4) {
                let alpha = u16::from(pixel[3]) * u16::from(color[3]) / 255;
                pixels.extend([
                    (u16::from(color[2]) * alpha / 255) as u8,
                    (u16::from(color[1]) * alpha / 255) as u8,
                    (u16::from(color[0]) * alpha / 255) as u8,
                    alpha as u8,
                ]);
            }
            pixels
        });
        let bitmap = device.upload(
            image.width,
            image.height,
            tinted.as_deref().unwrap_or(&image.bgra),
        )?;
        self.bytes += bytes;
        self.high_water = self.high_water.max(self.bytes);
        self.entries.insert(
            cache_key,
            Entry {
                owner: image.bgra.clone(),
                width: image.width,
                height: image.height,
                bitmap: bitmap.clone(),
                used: self.epoch,
            },
        );
        Ok(bitmap)
    }
}
