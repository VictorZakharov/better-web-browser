//! Standalone bitmap-publication command benchmark; compile with rustc -O.
#[path = "../../src/engine/image_decode/premultiply.rs"]
mod premultiply;

use std::{hint::black_box, time::Instant};

fn main() {
    for (name, alpha) in [("opaque", 255), ("partial", 127), ("mixed", 0)] {
        let source: Vec<u8> = (0..1_048_576_u32)
            .flat_map(|i| {
                [
                    i as u8,
                    (i >> 5) as u8,
                    (i >> 11) as u8,
                    if name == "mixed" {
                        (i >> 3) as u8
                    } else {
                        alpha
                    },
                ]
            })
            .collect();
        let expected: Vec<u8> = source
            .chunks_exact(4)
            .flat_map(|p| {
                let multiply = |c: u8| ((u16::from(c) * u16::from(p[3]) + 127) / 255) as u8;
                [multiply(p[2]), multiply(p[1]), multiply(p[0]), p[3]]
            })
            .collect();
        let mut samples = Vec::new();
        for iteration in 0..25 {
            // Allocation/copy and exhaustive result checking are outside the command clock.
            let mut pixels = source.clone();
            let start = Instant::now();
            premultiply::rgba_to_bgra(black_box(&mut pixels));
            let micros = start.elapsed().as_secs_f64() * 1e6;
            assert_eq!(pixels, expected);
            if iteration >= 5 {
                samples.push(micros);
            }
        }
        println!("{name}: microseconds={samples:?}");
    }
}
