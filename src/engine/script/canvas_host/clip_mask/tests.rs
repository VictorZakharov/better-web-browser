use super::*;

fn request(parts: Vec<Vec<[f64; 2]>>, rule: &str) -> Request {
    Request {
        width: 39,
        height: 27,
        bounds: [0, 0, 39, 27],
        rule: rule.into(),
        parts,
    }
}

// Independent point-in-polygon predicate, not another scanline implementation.
fn scalar(request: &Request, previous: Option<&[u8]>) -> Vec<u8> {
    let mut output = vec![0; (request.width as usize * request.height as usize).div_ceil(8)];
    for y in request.bounds[1]..request.bounds[3] {
        for x in request.bounds[0]..request.bounds[2] {
            let mut crossings = 0;
            let mut winding = 0;
            let py = f64::from(y) + 0.5;
            for part in &request.parts {
                if part.len() < 2 {
                    continue;
                }
                for index in 0..part.len() {
                    let [x0, y0] = part[index];
                    let [x1, y1] = part[(index + 1) % part.len()];
                    if (y0 <= py && y1 > py) || (y1 <= py && y0 > py) {
                        let intersection = x0 + (py - y0) * (x1 - x0) / (y1 - y0);
                        if intersection <= f64::from(x) + 0.5 {
                            crossings += 1;
                            winding += if y1 > y0 { 1 } else { -1 };
                        }
                    }
                }
            }
            let covered = if request.rule == "evenodd" {
                crossings % 2 != 0
            } else {
                winding != 0
            };
            let bit = y as usize * request.width as usize + x as usize;
            if covered && previous.is_none_or(|bits| bits[bit / 8] & (1 << (bit % 8)) != 0) {
                output[bit / 8] |= 1 << (bit % 8);
            }
        }
    }
    output
}

#[test]
fn sloping_compound_paths_match_independent_scalar_membership() {
    for seed in 0..40 {
        let mut parts = Vec::new();
        for part in 0..3 {
            let mut points = Vec::new();
            for index in 0..7 {
                points.push([
                    ((index * 13 + seed * 7 + part * 5) % 47) as f64 - 4.25,
                    ((index * 17 + seed * 3 + part * 11) % 35) as f64 - 3.75,
                ]);
            }
            parts.push(points);
        }
        let previous = (0..(39_usize * 27).div_ceil(8))
            .map(|index| (index * 37 + seed * 11) as u8)
            .collect::<Vec<_>>();
        for rule in ["nonzero", "evenodd"] {
            let request = request(parts.clone(), rule);
            assert_eq!(rasterize(&request, None).unwrap(), scalar(&request, None));
            assert_eq!(
                rasterize(&request, Some(&previous)).unwrap(),
                scalar(&request, Some(&previous))
            );
        }
    }
}

#[test]
fn half_pixel_boundaries_and_reversed_or_duplicate_contours_preserve_winding() {
    let contour = vec![[2.5, 3.5], [31.5, 3.5], [31.5, 23.5], [2.5, 23.5]];
    let reverse = contour.iter().copied().rev().collect::<Vec<_>>();
    for parts in [
        vec![contour.clone()],
        vec![contour.clone(), contour.clone()],
        vec![contour.clone(), reverse],
        vec![vec![[1.0, 1.0], [9.0, 9.0]]],
        vec![],
    ] {
        for rule in ["nonzero", "evenodd"] {
            let request = request(parts.clone(), rule);
            assert_eq!(rasterize(&request, None).unwrap(), scalar(&request, None));
        }
    }
}

#[test]
fn nested_clip_uses_the_immutable_previous_mask_and_clears_unused_tail_bits() {
    let mut request = request(
        vec![vec![[0.0, 0.0], [39.0, 0.0], [39.0, 27.0], [0.0, 27.0]]],
        "nonzero",
    );
    let previous = vec![255; (39_usize * 27).div_ceil(8)];
    request.bounds = [2, 3, 31, 23];
    let actual = rasterize(&request, Some(&previous)).unwrap();
    assert_eq!(actual, scalar(&request, Some(&previous)));
    assert_eq!(previous, vec![255; (39_usize * 27).div_ceil(8)]);
    assert_eq!(actual.last().unwrap() & 0xe0, 0);
    request.bounds = [0, 0, 39, 27];
    let full = rasterize(&request, Some(&previous)).unwrap();
    assert_eq!(*full.last().unwrap(), 0x1f);
}

#[test]
fn invalid_geometry_storage_and_work_decline_before_allocating_a_result() {
    let mut valid = request(
        vec![vec![[0.0, 0.0], [39.0, 0.0], [39.0, 27.0], [0.0, 27.0]]],
        "nonzero",
    );
    assert!(rasterize(&valid, Some(&[255])).is_none());
    valid.rule = "invalid".into();
    assert!(rasterize(&valid, None).is_none());
    valid.rule = "nonzero".into();
    valid.parts[0][0][0] = f64::NAN;
    assert!(rasterize(&valid, None).is_none());
    valid.parts[0][0][0] = 0.0;
    valid.bounds[2] = 40;
    assert!(rasterize(&valid, None).is_none());
    valid.bounds = [0, 0, 16384, 16384];
    valid.width = 16384;
    valid.height = 16384;
    assert!(rasterize(&valid, None).is_none());
    valid.width = 4096;
    valid.height = 4096;
    valid.bounds = [0, 0, 4096, 4096];
    assert!(
        rasterize(&valid, None).is_none(),
        "four-edge raster exceeds the existing work budget"
    );
    assert_eq!(paint(&[]), JsValue::Null);
}
