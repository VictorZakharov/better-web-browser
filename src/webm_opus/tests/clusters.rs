//! Unknown sizes are a live-container framing mechanism, not permission to
//! accept a truncated finite child or silently stop after a playable prefix.

use super::fixture_builder::*;

fn clustered(channels: u16, unknown_clusters: bool) -> Document {
    let mut document = Document::tone(channels);
    document.clusters = document
        .packets
        .iter()
        .enumerate()
        .map(|(index, packet)| {
            let mut result = cluster(
                index as u64 * 20,
                &[block(
                    packet,
                    0,
                    (index + 1 == document.packets.len()).then_some(13_500_000),
                )],
            );
            result.unknown = unknown_clusters;
            result
        })
        .collect();
    document
}

#[test]
fn finite_and_unknown_segments_clusters_have_identical_pcm_and_tail_trimming() {
    for channels in [1, 2] {
        let expected = decode(Document::tone(channels).bytes()).2;
        for unknown_segment in [false, true] {
            for unknown_clusters in [false, true] {
                let mut document = clustered(channels, unknown_clusters);
                document.unknown_segment = unknown_segment;
                let (count, frames, actual) = decode(document.bytes());
                assert_eq!((count, frames), (channels, 19_200));
                assert_eq!(
                    actual, expected,
                    "segment={unknown_segment} cluster={unknown_clusters}"
                );
            }
        }
    }
}

#[test]
fn unknown_cluster_ends_at_segment_metadata_instead_of_owning_following_clusters() {
    let mut document = clustered(1, true);
    document.unknown_segment = true;
    // Cues and Tags are Segment-level siblings; neither is an audio packet.
    document.clusters.insert(1, Field::master(0x1c53bb6b, &[]));
    document.clusters.insert(3, Field::master(0x1254c367, &[]));
    let expected = decode(Document::tone(1).bytes()).2;
    assert_eq!(decode(document.bytes()).2, expected);
}

#[test]
fn unknown_cluster_does_not_hide_truncated_packets_or_duplicate_timestamps() {
    let base = clustered(1, true);
    for unknown_segment in [false, true] {
        let mut document = base.clone();
        document.unknown_segment = unknown_segment;
        // A finite BlockGroup must be complete even when its parents are live.
        let last = document.clusters.last_mut().unwrap();
        last.data.pop();
        assert!(open(document.bytes(), crate::opus_audio::Limits::default()).is_err());
        let mut document = base.clone();
        document.unknown_segment = unknown_segment;
        document.clusters[0]
            .data
            .extend(Field::uint(0xe7, 0).bytes());
        rejects(document.bytes(), "Timestamp");
    }
}

#[test]
fn unknown_sizes_are_refused_for_blocks_tracks_and_metadata_children() {
    let base = Document::tone(1);
    let mut document = base.clone();
    let mut packet = block(&document.packets[0], 0, None);
    packet.unknown = true;
    document.clusters = vec![cluster(0, &[packet])];
    rejects(document.bytes(), "unknown size");
    for id in [0x4d80, 0x5741, 0x2ad7b1] {
        let mut document = base.clone();
        document
            .info
            .iter_mut()
            .find(|field| field.id == id)
            .unwrap()
            .unknown = true;
        rejects(document.bytes(), "unknown size");
    }
    let mut document = base.clone();
    document.clusters[0].data = [vec![0xe7, 0xff], 0_u64.to_be_bytes().to_vec()].concat();
    rejects(document.bytes(), "unknown size");
}

#[test]
fn misplaced_clusters_and_blocks_do_not_pass_as_ignored_binary_metadata() {
    let base = Document::tone(1);
    let mut document = base.clone();
    document.info.push(base.clusters[0].clone());
    rejects(document.bytes(), "Cluster is outside");
    let mut document = base.clone();
    document
        .extra_segment
        .push(block(&document.packets[0], 0, None));
    rejects(document.bytes(), "block is outside");
    let mut document = base.clone();
    document.clusters = vec![Field::master(
        0x1f43b675,
        &[block(&document.packets[0], 0, None), Field::uint(0xe7, 0)],
    )];
    rejects(document.bytes(), "timestamped Cluster");
}
