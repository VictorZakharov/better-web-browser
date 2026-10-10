//! Hoist unchanged center-sampling coordinates; retain exact coverage bytes.
use super::pixels;

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
