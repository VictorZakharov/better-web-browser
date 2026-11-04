//! Row subgrids: a grid item that adopts the rows it spans in its parent grid.
//!
//! The subgrid's items size the parent's rows, then the subgrid lays them out on the parent's
//! resolved rows and gutter, so siblings keep their parts aligned across rows they share.
//! The subgrid's own edge padding and a differing `row-gap` do not adjust those contributions yet,
//! and `grid-template-columns: subgrid` stays unadmitted until its tracks can take contributions.
//! https://drafts.csswg.org/css-grid-2/#subgrids
use super::*;

/// The parent's used rows across a subgrid's span, in used pixels.
#[derive(Clone, Debug, PartialEq)]
pub(in crate::engine::layout) struct SubgridRows {
    pub(super) heights: Vec<f32>,
    pub(super) gap: f32,
}

/// Whether `style` adopts the rows of a parent grid. Outside a grid the value acts as `none`.
pub(super) fn is_row_subgrid(style: &ComputedStyle) -> bool {
    style.display == Display::Grid
        && style
            .grid_template_rows
            .trim()
            .eq_ignore_ascii_case("subgrid")
}

/// A subgrid has no implicit rows: an item past its span lands in the span's last row.
/// https://drafts.csswg.org/css-grid-2/#subgrid-implicit
pub(super) fn clamp_rows(placements: &mut [GridItemPlacement], span: usize) {
    let span = span.max(1);
    for item in placements {
        item.row = item.row.min(span - 1);
        item.row_end = item.row_end.clamp(item.row + 1, span);
    }
}

impl<M: TextMeasurer> LayoutEngine<'_, M> {
    /// Each subgrid item's `(first row, row past the end, height)`, in the subgrid's own rows.
    /// https://drafts.csswg.org/css-grid-2/#subgrid-size-contribution
    pub(super) fn subgrid_row_contributions(
        &mut self,
        node: &NodeRef,
        outer_width: f32,
        span: usize,
    ) -> Vec<(usize, usize, f32)> {
        let style = self.styles.get(node);
        let insets = style
            .padding
            .resolve(outer_width, style.font_size)
            .horizontal()
            + style
                .border_width
                .resolve(outer_width, style.font_size)
                .horizontal()
            + style
                .margin
                .resolve(outer_width, style.font_size)
                .horizontal();
        let width = (outer_width - insets).max(0.0);
        let structure = self.grid_structure(node, style, width, Some(span));
        let columns = self.intrinsic_grid_columns(
            &structure.column_tracks,
            &structure.placements,
            width,
            structure.column_gap,
            style.font_size,
        );
        let mut contributions = Vec::with_capacity(structure.placements.len());
        for item in &structure.placements {
            let (_, cell_width) = grid_cell(&columns, structure.column_gap, item);
            let (_, item_width, used) = self.grid_item_inline_size(&item.node, 0.0, cell_width);
            let height = self
                .intrinsic_block_height(&item.node, item_width, None, used)
                .max(0.0);
            contributions.push((item.row, item.row_end, height));
        }
        contributions
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::layout::test_support::FixedMeasurer;

    fn bounds(html: &str, class: &str) -> RectF {
        let page = Page::parse(html, "https://example.test/");
        let output = layout_page(&page, 800.0, 600.0, &mut FixedMeasurer);
        let node = page
            .dom
            .elements_named("div")
            .find(|node| {
                node.attr("class")
                    .is_some_and(|classes| classes.split_whitespace().any(|name| name == class))
            })
            .unwrap();
        output.node_bounds[&node.id()]
    }

    /// Two overlaid cards whose parts share the rows, as a card picker keeps its layout steady.
    const SHARED_ROWS: &str = r#"<style>body{margin:0}
      main{display:grid;width:300px;height:400px;grid-template-rows:auto auto 1fr;row-gap:10px}
      section{grid-column:1;grid-row:1 / span 3;display:grid;grid-template-rows:subgrid}
      .tall{height:60px} .short{height:20px} .rest{height:30px}
      .pinned{height:30px;margin-top:auto}</style>
      <main>
        <section><div class="a1 short"></div><div class="a2 short"></div><div class="a3 rest"></div></section>
        <section><div class="b1 tall"></div><div class="b2 short"></div><div class="b3 pinned"></div></section>
      </main>"#;

    #[test]
    fn items_of_every_subgrid_size_the_rows_they_share() {
        for (class, y) in [
            ("a1", 0.0),
            ("b1", 0.0),
            ("a2", 70.0),
            ("b2", 70.0),
            ("a3", 100.0),
        ] {
            assert_eq!(bounds(SHARED_ROWS, class).y, y, "{class}");
        }
    }

    #[test]
    fn a_flexible_parent_row_reaches_the_subgrid_and_an_auto_margin_absorbs_it() {
        // The third row runs from 100 to the container's 400.
        assert_eq!(bounds(SHARED_ROWS, "b3").y, 370.0);
    }

    #[test]
    fn a_spanning_subgrid_item_spreads_over_the_parent_rows_it_crosses() {
        let html = r#"<style>body{margin:0}
          main{display:grid;width:300px;grid-template-rows:auto auto;row-gap:20px}
          section{grid-row:1 / span 2;display:grid;grid-template-rows:subgrid}
          .wide{grid-row:1 / span 2;height:100px} .after{height:1px}</style>
          <div class="outer"><main><section><div class="wide"></div></section></main></div>
          <div class="after"></div>"#;
        // 100px across both rows, including the gutter between them, and no more.
        assert_eq!(bounds(html, "outer").height, 100.0);
        assert_eq!(bounds(html, "after").y, 100.0);
    }

    #[test]
    fn a_changed_parent_row_reaches_a_subgrid_whose_own_size_did_not_change() {
        // Equal total height, different rows: the keys retained layouts use cannot tell them apart.
        let html = |first: &str, second: &str| {
            format!(
                r#"<style>body{{margin:0}}
              main{{display:grid;width:300px;height:100px;grid-template-rows:{first} {second}}}
              section{{grid-row:1 / span 2;display:grid;grid-template-rows:subgrid}}
              div{{height:1px}}</style>
              <main><section><div class="p1"></div><div class="p2"></div></section></main>"#
            )
        };
        assert_eq!(bounds(&html("20px", "80px"), "p2").y, 20.0);
        assert_eq!(bounds(&html("70px", "30px"), "p2").y, 70.0);
    }

    #[test]
    fn items_past_the_span_stay_in_its_last_row_rather_than_adding_rows() {
        let html = r#"<style>body{margin:0}
          main{display:grid;width:300px;grid-template-rows:40px 40px 40px}
          section{grid-row:1 / span 2;display:grid;grid-template-rows:subgrid}
          div{height:10px}</style>
          <main><section><div class="p1"></div><div class="p2"></div><div class="p3"></div></section></main>"#;
        assert_eq!(bounds(html, "p2").y, 40.0);
        assert_eq!(bounds(html, "p3").y, 40.0);
    }

    #[test]
    fn outside_a_grid_the_value_acts_as_none() {
        let html = r#"<style>body{margin:0}
          section{display:grid;grid-template-rows:subgrid;row-gap:5px}
          div{height:10px}</style>
          <section><div class="p1"></div><div class="p2"></div></section>"#;
        assert_eq!(bounds(html, "p2").y, 15.0);
    }

    #[test]
    fn column_subgrids_stay_unadmitted() {
        assert!(crate::engine::css::supports::supports_matches(
            "@supports (grid-template-rows: subgrid)"
        ));
        assert!(!crate::engine::css::supports::supports_matches(
            "@supports (grid-template-columns: subgrid)"
        ));
    }
}
