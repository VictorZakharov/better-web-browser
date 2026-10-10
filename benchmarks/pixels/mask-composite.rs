//! CPU-only mask command workload, using the production implementation.
//! Compile with rustc --edition=2024 -O; keep the executable and output on G:.
use std::{hint::black_box, time::Instant};
#[path = "../../src/windows_app/painting/masking/pixels.rs"]
mod pixels;
#[path = "../../src/windows_app/painting/masking/sampling.rs"]
mod sampling;

fn reference(destination: &mut [u8], rect: [i32; 4], full: [i32; 4], size: [u32; 2], mask: &[u8]) {
    let width = (rect[2] - rect[0]) as usize;
    for (index, pixel) in destination.chunks_exact_mut(4).enumerate() {
        let x = rect[0] + (index % width) as i32;
        let y = rect[1] + (index / width) as i32;
        let mx = ((f64::from(x - full[0]) + 0.5) * f64::from(size[0]) / f64::from(full[2] - full[0])).floor() as usize;
        let my = ((f64::from(y - full[1]) + 0.5) * f64::from(size[1]) / f64::from(full[3] - full[1])).floor() as usize;
        let alpha = mask[(my.min(size[1] as usize - 1) * size[0] as usize + mx.min(size[0] as usize - 1)) * 4 + 3];
        for channel in &mut pixel[..3] {
            *channel = ((u16::from(*channel) * u16::from(alpha) + 127) / 255) as u8;
        }
        pixel[3] = alpha;
    }
}

fn main() {
    let full = [11, 17, 1035, 1041];
    for (name, rect, size) in [
        ("unscaled", full, [1024, 1024]),
        ("scaled", full, [512, 384]),
        ("clipped", [34, 36, 1017, 1028], [1024, 1024]),
    ] {
        let bytes = (rect[2] - rect[0]) as usize * (rect[3] - rect[1]) as usize * 4;
        let input = (0..bytes).map(|i| (i * 37 + 53) as u8).collect::<Vec<_>>();
        let mask = (0..size[0] as usize * size[1] as usize * 4).map(|i| (i * 73 + 19) as u8).collect::<Vec<_>>();
        let mut expected = input.clone();
        let mut candidate = input.clone();
        reference(&mut expected, rect, full, size, &mask);
        sampling::apply(&mut candidate, rect, full, size, &mask);
        assert_eq!(candidate, expected, "pixels case={name}");
        println!("pixels case={name} differing_bytes=0");
        for rotation in 0..3 {
            for optimized in if rotation % 2 == 0 { [false, true] } else { [true, false] } {
                let mut output = input.clone();
                for _ in 0..5 {
                    if optimized { sampling::apply(&mut output, rect, full, size, &mask); }
                    else { reference(&mut output, rect, full, size, &mask); }
                }
                let mut samples = Vec::new();
                for _ in 0..20 {
                    output.copy_from_slice(&input);
                    let start = Instant::now();
                    if optimized { sampling::apply(black_box(&mut output), rect, full, size, black_box(&mask)); }
                    else { reference(black_box(&mut output), rect, full, size, black_box(&mask)); }
                    samples.push(start.elapsed().as_secs_f64() * 1000.0);
                    assert_eq!(output, expected, "case={name}, optimized={optimized}");
                }
                println!("case={name} rotation={rotation} optimized={optimized} samples_ms={samples:?}");
            }
        }
    }
}
