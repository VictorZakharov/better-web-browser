//! Optional CRCs cover encoded EBML bytes, including voids and child checksums.

use super::fixture_builder::*;

fn checksum(children: &[Field]) -> Field {
    let bytes = children.iter().flat_map(Field::bytes).collect::<Vec<_>>();
    Field::new(0xbf, crc32fast::hash(&bytes).to_le_bytes())
}

fn protect(children: &mut Vec<Field>) {
    children.insert(0, checksum(children));
}

fn protect_cluster(field: &mut Field) {
    assert_eq!(field.id, 0x1f43b675);
    let crc = Field::new(0xbf, crc32fast::hash(&field.data).to_le_bytes()).bytes();
    field.data.splice(0..0, crc);
}

#[test]
fn valid_nested_header_info_track_audio_and_cluster_crcs_preserve_actual_pcm() {
    for channels in [1, 2] {
        let mut document = Document::tone(channels);
        let expected = decode(document.bytes()).2;
        protect(&mut document.header);
        protect(&mut document.info);
        protect(&mut document.audio);
        // Track children are composed by Document::bytes, so cover their final
        // representation, including CodecPrivate and the protected Audio master.
        let mut track = document.track.clone();
        track.push(Field::new(0x63a2, document.head.clone()));
        track.push(Field::master(0xe1, &document.audio));
        document.track.insert(0, checksum(&track));
        for cluster in &mut document.clusters {
            protect_cluster(cluster);
        }
        assert_eq!(decode(document.bytes()).2, expected);
    }
}

#[test]
fn changing_even_ignored_parent_metadata_invalidates_its_crc() {
    let base = Document::tone(1);
    for target in [0x4d80, 0x5741] {
        let mut document = base.clone();
        protect(&mut document.info);
        document
            .info
            .iter_mut()
            .find(|field| field.id == target)
            .unwrap()
            .data[0] ^= 1;
        rejects(document.bytes(), "CRC-32");
    }
    let mut document = base.clone();
    document.header.push(Field::new(0xec, vec![0; 20]));
    protect(&mut document.header);
    document.header.last_mut().unwrap().data[19] = 1;
    rejects(document.bytes(), "CRC-32");
}

#[test]
fn damaged_compressed_payload_with_valid_framing_is_not_accepted_under_a_crc() {
    let mut document = Document::tone(1);
    protect_cluster(&mut document.clusters[0]);
    // The tail byte is still part of a finite Block; CRC verification happens
    // before the tolerant demuxer or predictive codec sees altered data.
    *document.clusters[0].data.last_mut().unwrap() ^= 1;
    rejects(document.bytes(), "CRC-32");
}

#[test]
fn invalid_crc_width_byte_order_ordering_and_duplicates_are_not_last_value_wins() {
    let base = Document::tone(1);
    for width in [0, 1, 3, 5, 8] {
        let mut document = base.clone();
        document.info.insert(0, Field::new(0xbf, vec![0; width]));
        rejects(document.bytes(), "CRC-32");
    }
    let mut document = base.clone();
    let mut crc = checksum(&document.info);
    crc.data.reverse();
    document.info.insert(0, crc);
    rejects(document.bytes(), "CRC-32");
    let mut document = base.clone();
    let crc = checksum(&document.info);
    document.info.push(crc);
    rejects(document.bytes(), "first child");
    let mut document = base.clone();
    protect(&mut document.info);
    // Recompute the first CRC so the only failure is duplicate placement.
    document.info.insert(1, Field::new(0xbf, [0; 4]));
    document.info[0] = checksum(&document.info[1..]);
    rejects(document.bytes(), "first child");
}

#[test]
fn unknown_cluster_crc_covers_only_that_cluster_not_its_following_sibling() {
    let mut document = Document::tone(1);
    let expected = decode(document.bytes()).2;
    let mut first = cluster(0, &[block(&document.packets[0], 0, None)]);
    first.unknown = true;
    protect_cluster(&mut first);
    let rest = document.packets[1..]
        .iter()
        .enumerate()
        .map(|(index, packet)| {
            block(
                packet,
                (index * 20) as i16,
                (index + 2 == document.packets.len()).then_some(13_500_000),
            )
        })
        .collect::<Vec<_>>();
    let mut second = cluster(20, &rest);
    protect_cluster(&mut second);
    document.clusters = vec![first, second];
    document.unknown_segment = true;
    assert_eq!(decode(document.bytes()).2, expected);
    *document.clusters[1].data.last_mut().unwrap() ^= 1;
    rejects(document.bytes(), "CRC-32");
}

#[test]
fn segment_crc_covers_the_entire_complete_document_including_child_checksums() {
    let document = Document::tone(1);
    let original = document.bytes();
    let mut budget = crate::encoded_audio::Budget::new(None);
    let mut output = Vec::new();
    super::super::elements::children(&original, &mut budget, |element, _| {
        if element.id == 0x18538067 {
            let mut children =
                Field::new(0xbf, crc32fast::hash(element.data).to_le_bytes()).bytes();
            children.extend_from_slice(element.data);
            output.extend(Field::new(element.id, children).bytes());
        } else {
            output.extend(Field::new(element.id, element.data).bytes());
        }
        Ok(())
    })
    .unwrap();
    assert_eq!(decode(output.clone()).2, decode(original).2);
    *output.last_mut().unwrap() ^= 1;
    rejects(output, "CRC-32");
}
