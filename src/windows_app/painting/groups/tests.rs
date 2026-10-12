use super::*;
use better_web_browser::engine::{RectF, affine::Affine2D};
use std::ptr::null_mut;

#[test]
fn excluded_nested_layers_do_not_allocate_or_modify_the_surviving_native_dc() {
    unsafe {
        let dc = CreateCompatibleDC(null_mut());
        assert!(!dc.is_null());
        let bitmap = CreateCompatibleBitmap(dc, 200, 150);
        assert!(!bitmap.is_null());
        let previous = SelectObject(dc, bitmap);
        assert!(!previous.is_null());
        let saved = SaveDC(dc);
        assert_ne!(saved, 0);
        IntersectClipRect(dc, 0, 0, 30, 30);
        let content = Rect {
            left: 0,
            top: 0,
            right: 200,
            bottom: 150,
        };
        let hidden = RectF {
            x: 100.0,
            y: 90.0,
            width: 60.0,
            height: 50.0,
        };
        let mut groups = PaintGroups::default();
        let images = HashMap::new();
        for item in [
            DisplayItem::BeginOpacity {
                bounds: hidden,
                opacity: 0.5,
            },
            DisplayItem::BeginTransform {
                bounds: hidden,
                matrix: Affine2D([1.0, 0.0, 0.0, 1.0, 13.0, 17.0]),
            },
            DisplayItem::BeginMask {
                bounds: hidden,
                url: "absent-mask".into(),
            },
            DisplayItem::EndMask { bounds: hidden },
            DisplayItem::EndTransform { bounds: hidden },
            DisplayItem::EndOpacity { bounds: hidden },
        ] {
            assert!(groups.handle(&item, dc, &images, 0, 0, 1.0, &content, &content));
            assert!(
                groups.layers.is_empty(),
                "excluded groups must not allocate a layer"
            );
            assert_eq!(groups.bytes, 0);
            assert_eq!(groups.dc(dc), dc);
        }
        assert_eq!(groups.skipped, 0, "nested boundaries must balance");
        assert!(!super::super::visibility::drawable(
            dc,
            Rect {
                left: 100,
                top: 90,
                right: 160,
                bottom: 140
            }
        ));
        assert!(super::super::visibility::drawable(
            dc,
            Rect {
                left: 4,
                top: 4,
                right: 20,
                bottom: 20
            }
        ));
        assert_ne!(RestoreDC(dc, saved), 0);
        assert!(super::super::visibility::drawable(
            dc,
            Rect {
                left: 100,
                top: 90,
                right: 160,
                bottom: 140
            }
        ));
        SelectObject(dc, previous);
        DeleteObject(bitmap);
        DeleteDC(dc);
    }
}
