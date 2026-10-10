use super::*;

fn reference(destination: &mut [u8], rect: [i32; 4], full: [i32; 4], size: [u32; 2], mask: &[u8]) {
    let width = (rect[2] - rect[0]) as usize;
    for (index, pixel) in destination.chunks_exact_mut(4).enumerate() {
        let x = rect[0] + (index % width) as i32;
        let y = rect[1] + (index / width) as i32;
        let mx = ((f64::from(x - full[0]) + 0.5) * f64::from(size[0])
            / f64::from(full[2] - full[0]))
        .floor() as usize;
        let my = ((f64::from(y - full[1]) + 0.5) * f64::from(size[1])
            / f64::from(full[3] - full[1]))
        .floor() as usize;
        let alpha = mask[(my.min(size[1] as usize - 1) * size[0] as usize
            + mx.min(size[0] as usize - 1))
            * 4
            + 3];
        for channel in &mut pixel[..3] {
            *channel = ((u16::from(*channel) * u16::from(alpha) + 127) / 255) as u8;
        }
        pixel[3] = alpha;
    }
}

#[test]
fn hoisted_sampling_matches_every_channel_for_scaled_clipped_and_translated_masks() {
    for width in 1..38 {
        for height in 1..20 {
            for size in [
                [1, 1],
                [2, 7],
                [width, height],
                [width + 1, height],
                [29, 13],
            ] {
                let full = [-11, 7, -11 + width as i32, 7 + height as i32];
                let rect = [
                    full[0] + width as i32 / 3,
                    full[1] + height as i32 / 3,
                    full[2] - width as i32 / 4,
                    full[3] - height as i32 / 4,
                ];
                let bytes = (rect[2] - rect[0]) as usize * (rect[3] - rect[1]) as usize * 4;
                let mask = (0..size[0] as usize * size[1] as usize * 4)
                    .map(|i| (i * 73 + 19) as u8)
                    .collect::<Vec<_>>();
                let mut candidate = (0..bytes).map(|i| (i * 37 + 53) as u8).collect::<Vec<_>>();
                let mut expected = candidate.clone();
                reference(&mut expected, rect, full, size, &mask);
                apply(&mut candidate, rect, full, size, &mask);
                assert_eq!(
                    candidate, expected,
                    "full={full:?}, rect={rect:?}, mask={size:?}"
                );
            }
        }
    }
}
