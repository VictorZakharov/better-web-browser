use super::super::raster;
use super::*;

fn source(width: u32, height: u32) -> String {
    format!(
        "<svg xmlns='http://www.w3.org/2000/svg' width='{width}' height='{height}' viewBox='0 0 700 330'><defs><linearGradient id='metal' x2='0' y2='1'><stop stop-color='#c5c2bb'/><stop offset='.45' stop-color='#494e55'/><stop offset='.6' stop-color='#fff'/><stop offset='1' stop-color='#555'/></linearGradient><filter id='glow' x='-20%' y='-40%' width='140%' height='180%'><feGaussianBlur stdDeviation='8'/></filter></defs><g fill='red' filter='url(#glow)'><path d='M20 30H180V100H85V230H300V300H20Z M350 30H650V100H415V230H650V300H350Z'/></g><g fill='url(#metal)' stroke='black' stroke-width='4'><path d='M20 30H180V100H85V230H300V300H20Z M350 30H650V100H415V230H650V300H350Z'/></g></svg>"
    )
}

#[test]
#[ignore = "isolated SVG raster command timing; run alone with --nocapture"]
fn measure_inline_svg_raster_at_used_viewport() {
    let natural_source = source(700, 330);
    let request = Request::new(140., 66., 1.25).unwrap();
    let natural_expected =
        super::super::super::decode_svg(natural_source.as_bytes(), "natural SVG reference")
            .unwrap();
    let used_expected = raster::render(&source(140, 66), &[], request).unwrap();
    assert_eq!((used_expected.width, used_expected.height), (175, 83));
    assert!(
        used_expected
            .bgra
            .chunks_exact(4)
            .filter(|p| p[3] != 0)
            .count()
            > 3000
    );
    for round in 0..4 {
        for used in if round % 2 == 0 {
            [false, true]
        } else {
            [true, false]
        } {
            for sample in 0..15 {
                let start = std::time::Instant::now();
                let actual = if used {
                    raster::render(&natural_source, &[], request).unwrap()
                } else {
                    super::super::super::decode_svg(
                        natural_source.as_bytes(),
                        "natural SVG command",
                    )
                    .unwrap()
                };
                let elapsed = start.elapsed().as_secs_f64() * 1000.;
                let expected = if used {
                    &used_expected
                } else {
                    &natural_expected
                };
                assert_eq!(
                    (actual.width, actual.height),
                    (expected.width, expected.height)
                );
                assert_eq!(actual.bgra, expected.bgra);
                if sample >= 3 {
                    println!(
                        "svg-viewport round={round} used={used} sample={} ms={elapsed:.4}",
                        sample - 3
                    );
                }
            }
        }
    }
}
