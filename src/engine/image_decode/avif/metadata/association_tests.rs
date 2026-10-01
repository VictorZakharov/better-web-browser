use super::*;
use test_support::*;

#[test]
fn properties_are_resolved_by_primary_item_id_not_definition_order() {
    let properties = [boxed(b"irot", &[1]), boxed(b"irot", &[3])];
    let bytes = metadata(
        7,
        &properties,
        &[associations(0, false, &[(2, vec![0x81]), (7, vec![0x82])])],
    );
    let transformed = parse(&bytes)
        .unwrap()
        .transform(image(), DecodeLimits::CANVAS, false)
        .unwrap();
    assert_eq!((transformed.width, transformed.height), (2, 3));
    assert_eq!(red_pixels(&transformed), [4, 1, 5, 2, 6, 3]);
}

#[test]
fn thirty_two_bit_item_ids_and_sixteen_bit_property_indices_are_supported() {
    let mut properties = vec![boxed(b"junk", &[]); 128];
    properties.push(boxed(b"imir", &[0]));
    let bytes = metadata(
        65537,
        &properties,
        &[associations(1, true, &[(65537, vec![0x8081])])],
    );
    let transformed = parse(&bytes)
        .unwrap()
        .transform(image(), DecodeLimits::CANVAS, false)
        .unwrap();
    assert_eq!(red_pixels(&transformed), [3, 2, 1, 6, 5, 4]);
}

#[test]
fn zero_property_indices_are_reserved_not_an_underflow() {
    let bytes = metadata(1, &[], &[associations(0, false, &[(1, vec![0, 0x80])])]);
    let transformed = parse(&bytes)
        .unwrap()
        .transform(image(), DecodeLimits::CANVAS, false)
        .unwrap();
    assert_eq!(red_pixels(&transformed), [1, 2, 3, 4, 5, 6]);
}

#[test]
fn out_of_range_associations_are_rejected_even_on_non_primary_items() {
    for item in [1, 2] {
        let bytes = metadata(
            1,
            &[boxed(b"junk", &[])],
            &[associations(0, false, &[(item, vec![2]), (1, vec![])])],
        );
        assert!(parse(&bytes).is_err());
    }
}

#[test]
fn duplicate_primary_entries_across_association_boxes_are_rejected() {
    let first = associations(0, false, &[(1, vec![])]);
    let bytes = metadata(1, &[], &[first.clone(), first]);
    assert!(parse(&bytes).unwrap_err().contains("duplicate"));
    let bytes = metadata(
        1,
        &[],
        &[associations(0, false, &[(1, vec![]), (1, vec![])])],
    );
    assert!(parse(&bytes).is_err());
}

#[test]
fn unknown_essential_properties_are_rejected_only_for_selected_primary_item() {
    let property = boxed(b"zzzz", &[1, 2, 3]);
    let optional = metadata(
        1,
        std::slice::from_ref(&property),
        &[associations(0, false, &[(1, vec![1])])],
    );
    assert!(parse(&optional).is_ok());
    let essential = metadata(
        1,
        std::slice::from_ref(&property),
        &[associations(0, false, &[(1, vec![0x81])])],
    );
    assert!(parse(&essential).unwrap_err().contains("essential"));
    let auxiliary = metadata(
        1,
        &[property],
        &[associations(0, false, &[(2, vec![0x81]), (1, vec![])])],
    );
    assert!(parse(&auxiliary).is_ok());
}

#[test]
fn absent_primary_associations_and_unsupported_versions_fail_closed() {
    let bytes = metadata(1, &[], &[associations(0, false, &[(2, vec![])])]);
    assert!(parse(&bytes).is_err());
    let bytes = metadata(1, &[], &[associations(2, false, &[(1, vec![])])]);
    assert!(parse(&bytes).is_err());
    let malformed = boxed(b"ipma", &full_box(0, 2, &[0, 0, 0, 0]));
    assert!(parse(&metadata(1, &[], &[malformed])).is_err());
}

#[test]
fn association_payloads_cannot_hide_trailing_bytes_or_excessive_counts() {
    let mut trailing = associations(0, false, &[(1, vec![])]);
    trailing.push(0);
    let length = trailing.len() as u32;
    trailing[..4].copy_from_slice(&length.to_be_bytes());
    assert!(parse(&metadata(1, &[], &[trailing])).is_err());
    let huge = boxed(b"ipma", &full_box(0, 0, &1025u32.to_be_bytes()));
    assert!(
        parse(&metadata(1, &[], &[huge]))
            .unwrap_err()
            .contains("too many")
    );
}

#[test]
fn required_container_boxes_are_unique_and_full_box_headers_are_checked() {
    let original = metadata(1, &[], &[associations(0, false, &[(1, vec![])])]);
    let duplicate = concatenate(&[original.clone(), original.clone()]);
    assert!(parse(&duplicate).is_err());
    for offset in [8usize, 9, 10, 11] {
        let mut invalid = original.clone();
        invalid[offset] = 1;
        assert!(parse(&invalid).is_err());
    }
    for size in 0..original.len() {
        assert!(parse(&original[..size]).is_err(), "prefix {size}");
    }
}

#[test]
fn extended_box_sizes_and_parent_relative_zero_sizes_are_validated() {
    let payload = [1, 2, 3];
    let mut extended = 1u32.to_be_bytes().to_vec();
    extended.extend_from_slice(b"junk");
    extended.extend_from_slice(&19u64.to_be_bytes());
    extended.extend_from_slice(&payload);
    let parsed = boxes(&extended).unwrap();
    assert_eq!(parsed[0].data, payload);
    let mut zero = 0u32.to_be_bytes().to_vec();
    zero.extend_from_slice(b"junk");
    zero.extend_from_slice(&payload);
    assert_eq!(boxes(&zero).unwrap()[0].data, payload);
    extended[8..16].copy_from_slice(&u64::MAX.to_be_bytes());
    assert!(boxes(&extended).is_err());
}

#[test]
fn box_count_budget_limits_metadata_traversal() {
    let single = boxed(b"junk", &[]);
    assert_eq!(boxes(&single.repeat(1024)).unwrap().len(), 1024);
    assert!(boxes(&single.repeat(1025)).is_err());
}
