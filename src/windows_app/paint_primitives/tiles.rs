//! Background tile placement shared by the GDI and GPU painters.
use better_web_browser::engine::RectF;

/// Upper bound on tiles painted for one background layer.
pub(in crate::windows_app) const MAX_TILES: usize = 4_096;

/// Tiles of one background layer that can intersect `clip`, in CSS px.
///
/// Each tile is `tile` offset by a whole number of tile sizes (CSS Backgrounds 3
/// §3.4), computed from its index rather than by accumulation so every painter
/// snaps the same CSS edges with `screen_rect`. A tile at the layer's origin
/// therefore covers the same device rows as a solid rect at the same position.
pub(in crate::windows_app) fn background_tiles(
    clip: RectF,
    tile: RectF,
    repeat_x: bool,
    repeat_y: bool,
) -> impl Iterator<Item = RectF> {
    let ((first_x, columns), (first_y, rows)) = grid(clip, tile, repeat_x, repeat_y);
    (first_y..first_y.saturating_add(rows))
        .flat_map(move |row| {
            (first_x..first_x.saturating_add(columns)).map(move |column| RectF {
                x: tile.x + column as f32 * tile.width,
                y: tile.y + row as f32 * tile.height,
                width: tile.width,
                height: tile.height,
            })
        })
        .take(MAX_TILES)
}

/// Number of tiles `background_tiles` would yield without the `MAX_TILES` cap.
pub(in crate::windows_app) fn background_tile_count(
    clip: RectF,
    tile: RectF,
    repeat_x: bool,
    repeat_y: bool,
) -> u64 {
    let ((_, columns), (_, rows)) = grid(clip, tile, repeat_x, repeat_y);
    (columns as u64).saturating_mul(rows as u64)
}

/// First tile index and tile count on each axis.
fn grid(clip: RectF, tile: RectF, repeat_x: bool, repeat_y: bool) -> ((i64, i64), (i64, i64)) {
    if tile.width <= 0.0 || tile.height <= 0.0 {
        return ((0, 0), (0, 0));
    }
    let axis = |repeat: bool, clip_start: f32, clip_end: f32, start: f32, size: f32| {
        if !repeat {
            return (0, 1);
        }
        let first = ((clip_start - start) / size).floor() as i64;
        let end = ((clip_end - start) / size).ceil() as i64;
        // Float-to-int casts saturate; keep the index arithmetic saturating too.
        (first, end.saturating_sub(first).max(1))
    };
    (
        axis(repeat_x, clip.x, clip.right(), tile.x, tile.width),
        axis(repeat_y, clip.y, clip.bottom(), tile.y, tile.height),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::windows_app::paint_primitives::screen_rect;

    fn rect(x: f32, y: f32, width: f32, height: f32) -> RectF {
        RectF {
            x,
            y,
            width,
            height,
        }
    }

    #[test]
    fn a_background_tile_snaps_to_the_same_device_rows_as_a_rect_at_its_position() {
        // At 125% a tile and a box at y=18 both start at 22.5 device px; one snapping
        // rule must put them on the same row.
        let tile = rect(18.0, 18.0, 20.0, 20.0);
        let edges = |rect| {
            let rect = screen_rect(rect, 0, 0, 1.25);
            (rect.left, rect.top, rect.right, rect.bottom)
        };
        let solid = edges(tile);
        for repeat in [false, true] {
            let tiles: Vec<_> =
                background_tiles(rect(0.0, 17.0, 60.0, 40.0), tile, repeat, false).collect();
            let origin = tiles.iter().find(|candidate| candidate.x == 18.0).unwrap();
            assert_eq!(edges(*origin), solid, "repeat {repeat}");
        }
    }

    #[test]
    fn repeated_tiles_are_placed_from_their_index_without_accumulated_error() {
        // CSS Backgrounds 3 §3.4: repeated tiles sit at whole multiples of the tile size.
        let tile = rect(0.0, 0.0, 0.3, 10.0);
        let tiles: Vec<_> =
            background_tiles(rect(0.0, 0.0, 100.0, 10.0), tile, true, false).collect();
        assert_eq!(tiles.len(), 334);
        assert_eq!(tiles.last().unwrap().x, 333.0 * 0.3);
        assert!(tiles.iter().all(|tile| tile.y == 0.0));
    }

    #[test]
    fn unrepeated_and_empty_tiles_paint_at_most_once() {
        let clip = rect(0.0, 0.0, 50.0, 50.0);
        assert_eq!(
            background_tiles(clip, rect(5.0, 5.0, 4.0, 4.0), false, false).count(),
            1
        );
        assert_eq!(
            background_tiles(clip, rect(5.0, 5.0, 0.0, 4.0), true, true).count(),
            0
        );
        let grid = background_tiles(clip, rect(5.0, 5.0, 10.0, 10.0), true, true).count();
        assert_eq!(grid, 6 * 6);
    }

    #[test]
    fn vanishingly_small_tiles_stay_within_the_tile_cap() {
        let clip = rect(-1.0e30, -1.0e30, 2.0e30, 2.0e30);
        let tile = rect(0.0, 0.0, 1.0e-30, 1.0e-30);
        assert_eq!(background_tiles(clip, tile, true, true).count(), MAX_TILES);
        assert!(background_tile_count(clip, tile, true, true) > MAX_TILES as u64);
    }
}
