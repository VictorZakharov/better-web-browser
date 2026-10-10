use super::*;

pub(super) fn property(style: &ComputedStyle, name: &str) -> Option<String> {
    let name = name.strip_prefix("-webkit-").unwrap_or(name);
    if name == "mask-image" {
        return Some(style.mask_image.css_text());
    }
    if let Some(suffix) = name.strip_prefix("mask-") {
        let mut view = style.clone();
        view.background_layers.clone_from(&style.mask_layers);
        return property(&view, &format!("background-{suffix}"));
    }
    let lengths = |values: &[Length]| {
        values
            .iter()
            .map(|v| lengths::serialize_computed_length(v.clone(), style.font_size))
            .collect::<Vec<_>>()
            .join(", ")
    };
    Some(match name {
        "background-position-x" => lengths(&style.background_layers.x),
        "background-position-y" => lengths(&style.background_layers.y),
        "background-position" => {
            let count = style
                .background_layers
                .x
                .len()
                .max(style.background_layers.y.len());
            (0..count)
                .map(|i| {
                    let x = &style.background_layers.x[i % style.background_layers.x.len()];
                    let y = &style.background_layers.y[i % style.background_layers.y.len()];
                    format!(
                        "{} {}",
                        lengths::serialize_computed_length(x.clone(), style.font_size),
                        lengths::serialize_computed_length(y.clone(), style.font_size)
                    )
                })
                .collect::<Vec<_>>()
                .join(", ")
        }
        "background-size" => style
            .background_layers
            .sizes
            .iter()
            .map(|size| match size {
                BackgroundSize::Auto => "auto".into(),
                BackgroundSize::Cover => "cover".into(),
                BackgroundSize::Contain => "contain".into(),
                BackgroundSize::Explicit { width, height } => format!(
                    "{} {}",
                    lengths::serialize_computed_length(width.clone(), style.font_size),
                    lengths::serialize_computed_length(height.clone(), style.font_size)
                ),
            })
            .collect::<Vec<_>>()
            .join(", "),
        "background-repeat" => style
            .background_layers
            .repeats
            .iter()
            .map(|repeat| match repeat {
                (true, true) => "repeat",
                (false, false) => "no-repeat",
                (true, false) => "repeat-x",
                (false, true) => "repeat-y",
            })
            .collect::<Vec<_>>()
            .join(", "),
        _ => return None,
    })
}
