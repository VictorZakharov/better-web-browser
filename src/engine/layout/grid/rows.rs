//! Resolve spanning contributions after the non-spanning intrinsic row sizes.
//! https://www.w3.org/TR/css-grid-1/#algo-spanning-items
use super::*;

pub(super) fn is_fixed(track: Option<&GridTrack>) -> bool {
    matches!(track, Some(GridTrack::Fixed(_)))
}

fn fraction(track: Option<&GridTrack>) -> f32 {
    match track {
        Some(GridTrack::Fraction(value)) => *value,
        Some(GridTrack::MinMax(_, maximum)) => fraction(Some(maximum)),
        _ => 0.0,
    }
}

pub(super) fn resolve(
    tracks: &[GridTrack],
    heights: &mut [f32],
    contributions: &[(usize, usize, f32)],
    gap: f32,
    available: Option<f32>,
) {
    let mut spans = contributions.to_vec();
    spans.sort_by_key(|&(start, end, _)| end - start);
    for &(start, end, height) in &spans {
        if end - start <= 1 || (start..end).any(|i| fraction(tracks.get(i)) > 0.0) {
            continue;
        }
        let extra =
            (height - heights[start..end].iter().sum::<f32>() - gap * (end - start - 1) as f32)
                .max(0.0);
        let growable = (start..end)
            .filter(|&i| !is_fixed(tracks.get(i)))
            .collect::<Vec<_>>();
        for &i in &growable {
            heights[i] += extra / growable.len() as f32;
        }
    }
    // Spanning items crossing flexible tracks contribute to the flex fraction, not
    // to the min-content headings alongside them. Indefinite grids retain content.
    let mut flex_size = 0.0_f32;
    for (i, &height) in heights.iter().enumerate() {
        let factor = fraction(tracks.get(i));
        if factor > 0.0 {
            flex_size = flex_size.max(height / factor.max(1.0));
        }
    }
    for &(start, end, height) in &spans {
        let factors: f32 = (start..end).map(|i| fraction(tracks.get(i))).sum();
        if factors == 0.0 {
            continue;
        }
        let nonflex: f32 = (start..end)
            .filter(|&i| fraction(tracks.get(i)) == 0.0)
            .map(|i| heights[i])
            .sum();
        flex_size =
            flex_size.max((height - nonflex - gap * (end - start - 1) as f32) / factors.max(1.0));
    }
    if let Some(available) = available {
        let factors: f32 = (0..heights.len()).map(|i| fraction(tracks.get(i))).sum();
        let nonflex: f32 = heights
            .iter()
            .enumerate()
            .filter(|(i, _)| fraction(tracks.get(*i)) == 0.0)
            .map(|(_, h)| h)
            .sum();
        if factors > 0.0 {
            flex_size = flex_size.max(
                (available - nonflex - gap * heights.len().saturating_sub(1) as f32)
                    / factors.max(1.0),
            );
        }
    }
    for (i, height) in heights.iter_mut().enumerate() {
        let factor = fraction(tracks.get(i));
        if factor > 0.0 {
            *height = height.max(factor * flex_size);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spanning_sidebar_does_not_expand_intrinsic_title_rows() {
        let page = Page::parse(
            r#"<style>body{margin:0}
          main{display:grid;width:600px;grid-template-columns:400px 200px;
            grid-template-rows:min-content min-content 1fr;row-gap:10px;
            grid-template-areas:'title rail' 'tabs rail' 'body rail'}
          h1{grid-area:title;height:40px;margin:0} nav{grid-area:tabs;height:30px}
          article{grid-area:body;height:100px} aside{grid-area:rail;height:600px}
        </style><main><h1>Title</h1><nav>Tabs</nav><article>Body</article><aside>Rail</aside></main>"#,
            "https://example.test/",
        );
        let output = layout_page(
            &page,
            800.0,
            600.0,
            &mut crate::engine::layout::test_support::FixedMeasurer,
        );
        let bounds = |tag| output.node_bounds[&page.dom.elements_named(tag).next().unwrap().id()];
        assert_eq!(bounds("nav").y, 50.0);
        assert_eq!(bounds("article").y, 90.0);
        assert_eq!(bounds("main").height, 600.0);
    }

    #[test]
    fn intrinsic_spans_respect_fixed_rows_and_flex_ratios() {
        let mut sizes = [20.0, 10.0, 0.0];
        resolve(
            &[
                GridTrack::Fixed(Length::Px(20.0)),
                GridTrack::Auto,
                GridTrack::Auto,
            ],
            &mut sizes,
            &[(0, 3, 100.0)],
            5.0,
            None,
        );
        assert_eq!(sizes, [20.0, 40.0, 30.0]);
        let mut sizes = [30.0, 0.0, 0.0];
        resolve(
            &[
                GridTrack::Auto,
                GridTrack::Fraction(2.0),
                GridTrack::Fraction(1.0),
            ],
            &mut sizes,
            &[(0, 3, 180.0)],
            0.0,
            None,
        );
        assert_eq!(sizes, [30.0, 100.0, 50.0]);
    }

    #[test]
    fn stretched_grid_items_publish_resolved_boxes_and_percentage_descendants() {
        let page = Page::parse(
            r#"<style>body{margin:0} main{display:grid;width:400px;grid-template-columns:200px 200px;
              grid-template-rows:min-content 1fr;grid-template-areas:'title rail' 'body rail'}
              h1{grid-area:title;height:40px;margin:0} article{grid-area:body;padding:5px;overflow:hidden}
              aside{grid-area:rail;height:200px} div{height:50%;background:red}
            </style><main><h1>Title</h1><article><div></div></article><aside>Rail</aside></main>"#,
            "https://example.test/",
        );
        let output = layout_page(
            &page,
            800.0,
            600.0,
            &mut crate::engine::layout::test_support::FixedMeasurer,
        );
        let bounds = |tag| output.node_bounds[&page.dom.elements_named(tag).next().unwrap().id()];
        assert_eq!(bounds("article").height, 160.0);
        assert_eq!(bounds("div").height, 75.0);
        assert_eq!(bounds("div").y, 45.0);
        assert!(output.items.iter().any(|item| matches!(item,
            DisplayItem::BeginClip { bounds } if bounds.height == 160.0)));
        let article = page.dom.elements_named("article").next().unwrap().id();
        assert_eq!(
            output
                .node_paint_order
                .iter()
                .filter(|id| **id == article)
                .count(),
            1
        );
    }
}
