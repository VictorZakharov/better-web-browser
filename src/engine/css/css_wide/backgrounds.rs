use super::*;
pub(super) fn copy(style: &mut ComputedStyle, source: &ComputedStyle, property: &str) -> bool {
    match property {
        "background" => {
            style.background_color = source.background_color;
            style.background_image.clone_from(&source.background_image);
            style.background_repeat_x = source.background_repeat_x;
            style.background_repeat_y = source.background_repeat_y;
            style.background_position_x = source.background_position_x.clone();
            style.background_position_y = source.background_position_y.clone();
            style.background_size = source.background_size.clone();
            style
                .background_layers
                .clone_from(&source.background_layers);
        }
        "background-color" => style.background_color = source.background_color,
        "background-image" => style.background_image.clone_from(&source.background_image),
        "mask" | "-webkit-mask" => {
            style.mask_image.clone_from(&source.mask_image);
            style.mask_layers.clone_from(&source.mask_layers);
        }
        "mask-image" | "-webkit-mask-image" => style.mask_image.clone_from(&source.mask_image),
        "mask-position" | "-webkit-mask-position" => {
            style.mask_layers.x.clone_from(&source.mask_layers.x);
            style.mask_layers.y.clone_from(&source.mask_layers.y);
        }
        "mask-size" | "-webkit-mask-size" => style
            .mask_layers
            .sizes
            .clone_from(&source.mask_layers.sizes),
        "mask-repeat" | "-webkit-mask-repeat" => style
            .mask_layers
            .repeats
            .clone_from(&source.mask_layers.repeats),
        "clip-path" => style.clip_path.clone_from(&source.clip_path),
        "background-repeat" => {
            style.background_repeat_x = source.background_repeat_x;
            style.background_repeat_y = source.background_repeat_y;
            style
                .background_layers
                .repeats
                .clone_from(&source.background_layers.repeats);
        }
        "background-position" => {
            style.background_position_x = source.background_position_x.clone();
            style.background_position_y = source.background_position_y.clone();
            style
                .background_layers
                .x
                .clone_from(&source.background_layers.x);
            style
                .background_layers
                .y
                .clone_from(&source.background_layers.y);
        }
        "background-position-x" => {
            style.background_position_x = source.background_position_x.clone();
            style
                .background_layers
                .x
                .clone_from(&source.background_layers.x);
        }
        "background-position-y" => {
            style.background_position_y = source.background_position_y.clone();
            style
                .background_layers
                .y
                .clone_from(&source.background_layers.y);
        }
        "background-size" => {
            style.background_size = source.background_size.clone();
            style
                .background_layers
                .sizes
                .clone_from(&source.background_layers.sizes);
        }
        _ => return false,
    }
    true
}
