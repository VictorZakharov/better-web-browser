use super::*;

fn key(value: u8) -> Key {
    Key {
        width: 1,
        height: 1,
        padding: 0,
        sigma: 1.0_f64.to_bits(),
        source: vec![value],
    }
}

fn alpha(value: f32) -> Arc<Alpha> {
    Arc::new(Alpha::from_raw(1, 1, vec![value]).unwrap())
}

#[test]
fn collisions_never_alias_different_source_alpha_or_kernel_metadata() {
    let mut cache = Cache::default();
    cache.insert(17, key(100), alpha(0.5));
    assert_eq!(cache.get(17, &key(100)).unwrap().as_raw(), &[0.5]);
    assert!(cache.get(17, &key(101)).is_none());
    let mut changed = key(100);
    changed.sigma = 2.0_f64.to_bits();
    assert!(cache.get(17, &changed).is_none());
    changed.sigma = 1.0_f64.to_bits();
    changed.padding = 3;
    assert!(cache.get(17, &changed).is_none());
    changed.padding = 0;
    changed.width = 2;
    assert!(cache.get(17, &changed).is_none());
}

#[test]
fn cached_masks_are_immutable_even_when_a_caller_changes_its_owned_arc_copy() {
    let mut cache = Cache::default();
    cache.insert(7, key(255), alpha(1.0));
    let mut first = cache.get(7, &key(255)).unwrap();
    Arc::make_mut(&mut first).as_mut()[0] = 0.0;
    assert_eq!(cache.get(7, &key(255)).unwrap().as_raw(), &[1.0]);
    assert_eq!(first.as_raw(), &[0.0]);
}

#[test]
fn entry_count_eviction_preserves_recent_hits_and_accounts_for_retained_bytes() {
    let mut cache = Cache::default();
    for value in 0..MAX_ENTRIES as u8 {
        cache.insert(u64::from(value), key(value), alpha(f32::from(value)));
    }
    cache.get(0, &key(0)).unwrap();
    cache.insert(100, key(100), alpha(100.0));
    assert!(
        cache.get(1, &key(1)).is_none(),
        "least-recently used entry was evicted"
    );
    assert!(cache.get(0, &key(0)).is_some());
    assert_eq!(cache.entries.len(), MAX_ENTRIES);
    assert_eq!(cache.bytes, MAX_ENTRIES * 5);
}

#[test]
fn byte_budget_eviction_and_oversized_admission_are_bounded() {
    let mut cache = Cache::default();
    let pixels = MAX_ENTRY_BYTES / 4 - 1;
    for value in 0..4 {
        cache.insert(
            value,
            key(value as u8),
            Arc::new(Alpha::from_raw(pixels as u32, 1, vec![0.0; pixels]).unwrap()),
        );
    }
    assert!(cache.bytes <= MAX_BYTES);
    assert_eq!(cache.entries.len(), 2);
    let before = cache.bytes;
    cache.insert(
        99,
        key(99),
        Arc::new(Alpha::from_raw((pixels + 2) as u32, 1, vec![0.0; pixels + 2]).unwrap()),
    );
    assert_eq!(cache.bytes, before);
    assert!(cache.get(99, &key(99)).is_none());
}

#[test]
fn repeated_large_mask_reuses_the_exact_image_rs_gaussian_without_sharing_paint() {
    KERNELS.with(|cache| *cache.borrow_mut() = Cache::default());
    let (width, height, padding) = (300, 240, 11);
    let mut source = vec![0; width as usize * height as usize];
    for (index, pixel) in source.iter_mut().enumerate() {
        *pixel = ((index * 37) % 256) as u8;
    }
    let first = gaussian(source.clone(), width, height, padding, 2.5);
    let second = gaussian(source.clone(), width, height, padding, 2.5);
    assert!(
        Arc::ptr_eq(&first, &second),
        "a cache hit has no kernel allocation or clone"
    );
    source[113] ^= 1;
    let changed = gaussian(source.clone(), width, height, padding, 2.5);
    assert!(!Arc::ptr_eq(&first, &changed));
    let mut reference = Alpha::new(width + padding * 2, height + padding * 2);
    for y in 0..height {
        for x in 0..width {
            reference.put_pixel(
                x + padding,
                y + padding,
                Luma([f32::from(source[(y * width + x) as usize]) / 255.0]),
            );
        }
    }
    assert_eq!(
        changed.as_raw(),
        image::imageops::blur(&reference, 2.5).as_raw()
    );
}
