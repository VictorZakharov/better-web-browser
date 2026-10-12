use super::*;

#[test]
fn refreshed_source_pixels_invalidate_reflected_variants_before_reusing_the_native_bitmap() {
    unsafe {
        let initial = patterned(97, 71);
        let changed = super::image(97, 71, [17, 31, 53, 127]);
        let expected = Target::new(&super::image(640, 400, [77; 4]));
        let actual = Target::new(&super::image(640, 400, [77; 4]));
        let rect = Rect {
            left: 30,
            top: 20,
            right: 287,
            bottom: 149,
        };
        let matrix = [-1.25, 0.0, 0.0, 1.25, 513.125, 17.75];
        setup(expected.dc, matrix, true);
        setup(actual.dc, matrix, true);
        let plan = Reflection::new(actual.dc, rect).unwrap();
        let mut bitmaps = crate::windows_app::rendering_resources::ImageBitmaps::default();
        let source = bitmaps.get_or_create("changing-canvas", &initial, actual.dc);
        assert!(!source.is_null());
        assert!(
            bitmaps
                .reflected_image("changing-canvas", &initial, source, plan, actual.dc)
                .is_none()
        );
        assert!(
            bitmaps
                .reflected_image("changing-canvas", &initial, source, plan, actual.dc)
                .is_some()
        );
        assert_eq!(bitmaps.scaled.entries.len(), 1);
        assert!(bitmaps.update("changing-canvas", &changed));
        assert!(bitmaps.scaled.entries.is_empty());
        assert!(bitmaps.scaled.pending.is_empty());
        let refreshed = bitmaps.get_or_create("changing-canvas", &changed, actual.dc);
        assert_eq!(
            refreshed, source,
            "same-size DIB reuse must not reuse baked pixels"
        );
        assert!(
            bitmaps
                .reflected_image("changing-canvas", &changed, refreshed, plan, actual.dc)
                .is_none()
        );
        let baked = bitmaps
            .reflected_image("changing-canvas", &changed, refreshed, plan, actual.dc)
            .unwrap();
        let reference_source = Bitmap::create(&changed, &changed.bgra, null_mut()).unwrap();
        paint_alpha_bitmap(expected.dc, reference_source.handle, &changed, &rect);
        let identity = crate::windows_app::painting::affine::IdentityScope::new(actual.dc).unwrap();
        paint_alpha_bitmap_size(
            actual.dc,
            baked.handle,
            plan.bounds().width() as u32,
            plan.bounds().height() as u32,
            &plan.bounds(),
        );
        drop(identity);
        assert_eq!(pixels(&actual.bitmap), pixels(&expected.bitmap));
        bitmaps.remove("changing-canvas");
        assert!(bitmaps.scaled.entries.is_empty() && bitmaps.scaled.pending.is_empty());
        assert_eq!(bitmaps.scaled.bytes, 0);
    }
}

#[test]
fn horizontal_variants_share_the_existing_bounded_scaled_cache_and_invalidate_together() {
    unsafe {
        let image = patterned(97, 71);
        let source = Bitmap::create(&image, &image.bgra, null_mut()).unwrap();
        let target = Target::new(&super::image(640, 400, [77; 4]));
        let mut cache = Cache {
            byte_limit: 512 * 1024,
            entry_limit: 3,
            pending_limit: 5,
            ..Cache::default()
        };
        let rect = Rect {
            left: 30,
            top: 20,
            right: 287,
            bottom: 149,
        };
        for index in 0..12 {
            setup(
                target.dc,
                [-1.25, 0.0, 0.0, 1.25, 513.125 + index as f32 / 8.0, 17.75],
                false,
            );
            let plan = Reflection::new(target.dc, rect).unwrap();
            assert!(
                cache
                    .reflected("same-image", &image, source.handle, plan, target.dc)
                    .is_none()
            );
            assert!(
                cache
                    .reflected("same-image", &image, source.handle, plan, target.dc)
                    .is_some()
            );
            assert!(cache.entries.len() <= 3 && cache.bytes <= 512 * 1024);
            assert_eq!(
                cache.bytes,
                cache.entries.iter().map(|v| v.bitmap.bytes).sum::<usize>()
            );
        }
        let identity = crate::windows_app::painting::affine::IdentityScope::new(target.dc).unwrap();
        assert!(
            cache
                .get("same-image", &image, source.handle, &rect, target.dc)
                .is_none()
        );
        assert!(
            cache
                .get("same-image", &image, source.handle, &rect, target.dc)
                .is_some()
        );
        drop(identity);
        assert!(cache.entries.iter().any(|v| v.key.reflection.is_none()));
        assert!(cache.entries.iter().any(|v| v.key.reflection.is_some()));
        cache.remove("same-image");
        assert!(cache.entries.is_empty() && cache.pending.is_empty());
        assert_eq!(cache.bytes, 0);
    }
}

#[test]
fn reflected_metadata_and_one_off_admission_remain_bounded() {
    unsafe {
        let image = patterned(97, 71);
        let source = Bitmap::create(&image, &image.bgra, null_mut()).unwrap();
        let target = Target::new(&super::image(640, 400, [77; 4]));
        setup(target.dc, [-1.25, 0.0, 0.0, 1.25, 513.125, 17.75], false);
        let rect = Rect {
            left: 30,
            top: 20,
            right: 287,
            bottom: 149,
        };
        let plan = Reflection::new(target.dc, rect).unwrap();
        let mut cache = Cache {
            pending_limit: 5,
            ..Cache::default()
        };
        for index in 0..100 {
            assert!(
                cache
                    .reflected(
                        &format!("one-off-{index}"),
                        &image,
                        source.handle,
                        plan,
                        target.dc
                    )
                    .is_none()
            );
            assert!(cache.pending.len() <= 5 && cache.entries.is_empty());
            assert_eq!(cache.bytes, 0);
        }
        for _ in 0..3 {
            assert!(
                cache
                    .reflected(
                        &"x".repeat(MAX_SOURCE_KEY_BYTES + 1),
                        &image,
                        source.handle,
                        plan,
                        target.dc
                    )
                    .is_none()
            );
        }
        assert_eq!(cache.pending.len(), 5);
        cache.clear();
        assert!(cache.pending.is_empty() && cache.entries.is_empty());
    }
}
