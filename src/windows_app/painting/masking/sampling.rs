//! Hoist unchanged center-sampling coordinates; retain exact coverage bytes.
use super::pixels;

pub(super) fn apply_affine(
    destination: &mut [u8],
    rect: super::Rect,
    full: super::Rect,
    mask: &super::DecodedImage,
    matrix: better_web_browser::engine::affine::Affine2D,
) {
    let Some(inverse) = matrix.inverse() else {
        destination.fill(0);
        return;
    };
    if rect.width() <= 0 || full.width() <= 0 || full.height() <= 0 {
        destination.fill(0);
        return;
    }
    let width = rect.width() as usize;
    if inverse.0[1] == 0.0 && inverse.0[2] == 0.0 {
        axis_aligned(destination, rect, full, mask, inverse);
        return;
    }
    general_affine(destination, rect, full, mask, inverse, width);
}

fn general_affine(
    destination: &mut [u8],
    rect: super::Rect,
    full: super::Rect,
    mask: &super::DecodedImage,
    inverse: better_web_browser::engine::affine::Affine2D,
    width: usize,
) {
    let mut sampled = vec![0u8; width * 4];
    for (row_index, row) in destination.chunks_exact_mut(width * 4).enumerate() {
        let y = f64::from(rect.top) + row_index as f64 + 0.5;
        for (column, pixel) in sampled.chunks_exact_mut(4).enumerate() {
            let (x, y) = inverse.point(f64::from(rect.left) + column as f64 + 0.5, y);
            pixel[3] = if x < f64::from(full.left)
                || y < f64::from(full.top)
                || x >= f64::from(full.right)
                || y >= f64::from(full.bottom)
            {
                0
            } else {
                let mx = ((x - f64::from(full.left)) * f64::from(mask.width)
                    / f64::from(full.width()))
                .floor() as usize;
                let my = ((y - f64::from(full.top)) * f64::from(mask.height)
                    / f64::from(full.height()))
                .floor() as usize;
                mask.bgra[(my.min(mask.height as usize - 1) * mask.width as usize
                    + mx.min(mask.width as usize - 1))
                    * 4
                    + 3]
            };
        }
        pixels::apply(row, &sampled);
    }
}

// A diagonal inverse makes x independent of the row and y independent of the
// column. Hoist only that work; preserve inverse.point's operation order and
// center sampling instead of accumulating approximate floating-point steps.
fn axis_aligned(
    destination: &mut [u8],
    rect: super::Rect,
    full: super::Rect,
    mask: &super::DecodedImage,
    inverse: better_web_browser::engine::affine::Affine2D,
) {
    let width = rect.width() as usize;
    let x0 = f64::from(rect.left) + 0.5;
    let y0 = f64::from(rect.top) + 0.5;
    let columns = (0..width)
        .map(|column| {
            let (x, _) = inverse.point(f64::from(rect.left) + column as f64 + 0.5, y0);
            mask_coordinate(x, full.left, full.right, mask.width)
        })
        .collect::<Vec<_>>();
    let mut sampled = vec![0u8; width * 4];
    for (row_index, row) in destination.chunks_exact_mut(width * 4).enumerate() {
        let (_, y) = inverse.point(x0, f64::from(rect.top) + row_index as f64 + 0.5);
        let Some(my) = mask_coordinate(y, full.top, full.bottom, mask.height) else {
            row.fill(0);
            continue;
        };
        let start = my * mask.width as usize * 4;
        let mask_row = &mask.bgra[start..start + mask.width as usize * 4];
        for (pixel, column) in sampled.chunks_exact_mut(4).zip(&columns) {
            pixel[3] = column.map_or(0, |x| mask_row[x * 4 + 3]);
        }
        pixels::apply(row, &sampled);
    }
}

fn mask_coordinate(value: f64, start: i32, end: i32, size: u32) -> Option<usize> {
    (value >= f64::from(start) && value < f64::from(end)).then(|| {
        (((value - f64::from(start)) * f64::from(size) / f64::from(end - start)).floor() as usize)
            .min(size as usize - 1)
    })
}

pub(super) fn apply(
    destination: &mut [u8],
    rect: [i32; 4],
    full: [i32; 4],
    size: [u32; 2],
    mask: &[u8],
) {
    let [left, top, right, _] = rect;
    let [full_left, full_top, full_right, full_bottom] = full;
    let [mask_width, mask_height] = size;
    let full_width = full_right - full_left;
    let full_height = full_bottom - full_top;
    let width = (right - left) as usize;
    // Keep the old expression/order instead of accumulating floating-point steps.
    let columns = (full_width != mask_width as i32).then(|| {
        (0..width)
            .map(|x| {
                let x = left + x as i32;
                (((f64::from(x - full_left) + 0.5) * f64::from(mask_width) / f64::from(full_width))
                    .floor() as usize)
                    .min(mask_width as usize - 1)
            })
            .collect::<Vec<_>>()
    });
    let mut sampled = columns.as_ref().map(|_| vec![0u8; width * 4]);
    for (row_index, row) in destination.chunks_exact_mut(width * 4).enumerate() {
        let y = top + row_index as i32;
        let my = ((f64::from(y - full_top) + 0.5) * f64::from(mask_height) / f64::from(full_height))
            .floor() as usize;
        let start = my.min(mask_height as usize - 1) * mask_width as usize * 4;
        let mask_row = &mask[start..start + mask_width as usize * 4];
        if let Some(columns) = &columns {
            let sampled = sampled.as_mut().unwrap();
            for (pixel, x) in sampled.chunks_exact_mut(4).zip(columns) {
                pixel[3] = mask_row[x * 4 + 3];
            }
            pixels::apply(row, sampled);
        } else {
            let start = (left - full_left) as usize * 4;
            pixels::apply(row, &mask_row[start..start + width * 4]);
        }
    }
}

#[cfg(test)]
#[path = "sampling/tests.rs"]
mod tests;

#[cfg(test)]
#[path = "sampling/affine_tests.rs"]
mod affine_tests;
