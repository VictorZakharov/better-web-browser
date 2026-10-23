use super::*;

fn rect(index: usize, red: u8) -> DisplayItem {
    DisplayItem::SolidRect {
        rect: RectF {
            x: index as f32,
            y: 0.0,
            width: 1.0,
            height: 1.0,
        },
        color: Color::rgb(red, 0, 0),
        radius: 0.0,
    }
}

fn items(count: usize) -> Vec<DisplayItem> {
    (0..count).map(|index| rect(index, 0)).collect()
}

fn encode(items: &[DisplayItem], base: Option<ItemBase<'_>>) -> Vec<u8> {
    let mut writer = WireWriter::new();
    encode_items(&mut writer, items, base).unwrap();
    writer.finish()
}

fn decode(bytes: &[u8], base: Option<ItemBase<'_>>) -> Result<Vec<DisplayItem>, ProtocolError> {
    let mut reader = WireReader::new(bytes);
    let items = decode_items(&mut reader, base)?;
    reader.finish()?;
    Ok(items)
}

/// Encodes `new` against `old` and checks the decoder rebuilds it; returns the encoded size.
fn round_trip(old: &[DisplayItem], new: &[DisplayItem]) -> usize {
    let base = ItemBase {
        revision: 4,
        items: old,
    };
    let bytes = encode(new, Some(base));
    assert_eq!(decode(&bytes, Some(base)).unwrap(), new);
    bytes.len()
}

#[test]
fn small_changes_send_only_the_changed_items() {
    let old = items(1000);
    let full = encode(&old, None).len();
    assert!(
        round_trip(&old, &old) < 32,
        "an unchanged list sends no items"
    );
    let mut recolored = old.clone();
    for index in [3, 4, 500, 998] {
        recolored[index] = rect(index, 200);
    }
    assert!(round_trip(&old, &recolored) < full / 50);
    let mut inserted = old.clone();
    inserted.splice(10..12, [rect(10, 9), rect(11, 9), rect(12, 9)]);
    assert!(round_trip(&old, &inserted) < full / 50);
    let mut removed = old.clone();
    removed.drain(990..);
    assert!(round_trip(&old, &removed) < 32);
    assert!(round_trip(&old, &[]) < 32);
}

#[test]
fn mostly_changed_lists_are_sent_whole() {
    let old = items(100);
    let new = (0..100).map(|index| rect(index, 1)).collect::<Vec<_>>();
    let base = ItemBase {
        revision: 1,
        items: &old,
    };
    let bytes = encode(&new, Some(base));
    assert_eq!(bytes[0], 0, "full encoding");
    assert_eq!(decode(&bytes, None).unwrap(), new);
}

#[test]
fn edits_need_the_base_they_were_made_against() {
    let old = items(100);
    let mut new = old.clone();
    new[50] = rect(50, 1);
    let bytes = encode(
        &new,
        Some(ItemBase {
            revision: 7,
            items: &old,
        }),
    );
    assert!(decode(&bytes, None).is_err());
    let stale = ItemBase {
        revision: 6,
        items: &old,
    };
    assert!(decode(&bytes, Some(stale)).is_err());
}

#[test]
fn malformed_edits_are_rejected() {
    let old = items(10);
    let base = Some(ItemBase {
        revision: 1,
        items: &old,
    });
    // (start, removed) pairs: past the end, overlapping the previous edit, overflowing.
    for edits in [vec![(8, 3)], vec![(4, 2), (5, 1)], vec![(u32::MAX, 2)]] {
        let mut writer = WireWriter::new();
        writer.u8(1);
        writer.u64(1);
        writer.u32(edits.len() as u32);
        for (start, removed) in edits {
            writer.u32(start);
            writer.u32(removed);
            writer.u32(0);
        }
        assert!(decode(&writer.finish(), base).is_err());
    }
}
