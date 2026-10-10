//! Background image layers share URL discovery and generated-image ownership.
use super::*;
pub(crate) mod gradient;
pub(crate) mod layers;
pub use gradient::Gradient;
pub use layers::BackgroundLayers;
#[cfg(test)]
mod tests;

#[derive(Debug, Clone, PartialEq)]
pub enum ImageSource {
    None,
    Url(String),
    Gradient(Arc<Gradient>),
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct BackgroundImages(pub Vec<ImageSource>);

impl BackgroundImages {
    pub fn is_none(&self) -> bool {
        self.0
            .iter()
            .all(|image| matches!(image, ImageSource::None))
    }
    pub fn is_some(&self) -> bool {
        !self.is_none()
    }
    pub fn as_ref(&self) -> Option<&String> {
        self.0.iter().find_map(|image| match image {
            ImageSource::Url(url) => Some(url),
            _ => None,
        })
    }
    pub fn as_deref(&self) -> Option<&str> {
        self.as_ref().map(String::as_str)
    }
    pub(crate) fn urls(&self) -> impl Iterator<Item = &String> {
        self.0.iter().filter_map(|image| match image {
            ImageSource::Url(url) => Some(url),
            _ => None,
        })
    }
    pub(crate) fn css_text(&self) -> String {
        if self.0.is_empty() {
            return "none".into();
        }
        self.0
            .iter()
            .map(|image| match image {
                ImageSource::None => "none".into(),
                ImageSource::Url(url) => {
                    let mut quoted = String::new();
                    let _ = cssparser::serialize_string(url, &mut quoted);
                    format!("url({quoted})")
                }
                ImageSource::Gradient(gradient) => gradient.css_text(),
            })
            .collect::<Vec<_>>()
            .join(", ")
    }
}

pub(crate) fn parse(value: &str, base_url: &str, shorthand: bool) -> Option<BackgroundImages> {
    if value.len() > 16_384 {
        return None;
    }
    let mut layers = Vec::new();
    for layer in split_css_top_level(value, ',') {
        if layers.len() == 32 {
            return None;
        }
        let parts = borrowed_components(layer)?;
        if parts.is_empty() {
            return None;
        }
        let mut found = None;
        for part in parts {
            let image = if ident(part).as_deref() == Some("none") {
                Some(ImageSource::None)
            } else if let Some(gradient) = Gradient::parse(part) {
                Some(ImageSource::Gradient(Arc::new(gradient)))
            } else {
                let mut source = ParserInput::new(part);
                let mut parser = Parser::new(&mut source);
                if let Ok(url) = parser.expect_url() {
                    let url = url.to_string();
                    parser.expect_exhausted().ok()?;
                    if url.is_empty() || url.starts_with('#') {
                        return None;
                    }
                    Some(ImageSource::Url(if base_url.is_empty() {
                        url
                    } else {
                        crate::navigation::resolve_resource_url(base_url, &url)?
                    }))
                } else {
                    if !shorthand
                        || function(part).is_some_and(|(name, _)| name.ends_with("gradient"))
                    {
                        return None;
                    }
                    None
                }
            };
            if let Some(image) = image {
                if found.is_some() {
                    return None;
                }
                found = Some(image);
            }
        }
        layers.push(found.unwrap_or(ImageSource::None));
    }
    if layers.len() == 1 && matches!(layers[0], ImageSource::None) {
        layers.clear();
    }
    Some(BackgroundImages(layers))
}
