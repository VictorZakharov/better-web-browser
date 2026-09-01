use super::{Page, PageResource};
use crate::limits::MAX_EMBEDDED_IMAGE_URL_BYTES;
use data_url::DataUrl;

impl Page {
    pub(super) fn install_embedded_images(&mut self) {
        let urls = self
            .resources
            .iter()
            .filter_map(|resource| match resource {
                PageResource::Image { url }
                    if url.starts_with("data:") && !self.images.contains_key(url) =>
                {
                    Some(url.clone())
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        for url in urls {
            let Ok(bytes) = decode_embedded_image(&url) else {
                continue;
            };
            let _ = self.add_image(url, &bytes);
        }
    }
}

fn decode_embedded_image(url: &str) -> Result<Vec<u8>, String> {
    if url.len() > MAX_EMBEDDED_IMAGE_URL_BYTES {
        return Err("embedded image URL is too large".into());
    }
    let data = DataUrl::process(url).map_err(|error| error.to_string())?;
    if data.mime_type().type_ != "image" {
        return Err("embedded resource is not an image".into());
    }
    let (bytes, _) = data
        .decode_to_vec()
        .map_err(|error| format!("decode embedded image: {error:?}"))?;
    Ok(bytes)
}
