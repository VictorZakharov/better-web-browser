//! Display items sent as edits to the previous presentation's items.
//
// A presentation carried every display item, about 200 bytes each with glyph positions, so a
// frame that changed one word still encoded, sent and decoded megabytes. The broker reads every
// presentation of a document in order before it coalesces or forwards them, so the renderer can
// send the items as edits to the list it sent last (the base) and the broker, which kept that
// list, rebuilds the whole one. Edits name ranges of the base to replace; a mismatched base
// revision is a protocol error, and a renderer without a base sends the full list.
use super::*;
use crate::engine::same_display_item;

/// Replace `removed` base items starting at `start` with the next `inserted` encoded items.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Edit {
    start: usize,
    removed: usize,
    inserted: usize,
}

/// Items of the previous presentation of the same document, as the codec saw them.
#[derive(Clone, Copy, Debug)]
pub struct ItemBase<'a> {
    pub revision: u64,
    pub items: &'a [DisplayItem],
}

pub(super) fn encode_items(
    writer: &mut WireWriter,
    items: &[DisplayItem],
    base: Option<ItemBase<'_>>,
) -> Result<(), ProtocolError> {
    let Some((base, edits)) = base.and_then(|base| Some((base, edits(base.items, items)?))) else {
        writer.u8(0);
        writer.u32(items.len() as u32);
        for item in items {
            encode_item(writer, item)?;
        }
        return Ok(());
    };
    writer.u8(1);
    writer.u64(base.revision);
    writer.u32(edits.len() as u32);
    let mut offset = 0_isize;
    for edit in edits {
        writer.u32(edit.start as u32);
        writer.u32(edit.removed as u32);
        writer.u32(edit.inserted as u32);
        // Inserted items sit at the same place in the new list, shifted by earlier edits.
        let first = (edit.start as isize + offset) as usize;
        for item in &items[first..first + edit.inserted] {
            encode_item(writer, item)?;
        }
        offset += edit.inserted as isize - edit.removed as isize;
    }
    Ok(())
}

pub(super) fn decode_items(
    reader: &mut WireReader<'_>,
    base: Option<ItemBase<'_>>,
) -> Result<Vec<DisplayItem>, ProtocolError> {
    let limit = MAX_DOM_NODES * 4;
    match reader.u8()? {
        0 => {
            let count = bounded_count(reader.u32()?, limit, "display items")?;
            let mut items = Vec::with_capacity(count);
            for _ in 0..count {
                items.push(decode_item(reader)?);
            }
            Ok(items)
        }
        1 => {
            let revision = reader.u64()?;
            let base = base
                .filter(|base| base.revision == revision)
                .ok_or(ProtocolError::InvalidPayload("display item base revision"))?;
            let count = bounded_count(reader.u32()?, limit, "display item edits")?;
            let mut items = Vec::with_capacity(base.items.len());
            let mut copied = 0;
            for _ in 0..count {
                let start = reader.u32()? as usize;
                let removed = reader.u32()? as usize;
                let inserted = bounded_count(reader.u32()?, limit, "display item edit")?;
                let end = start
                    .checked_add(removed)
                    .filter(|&end| start >= copied && end <= base.items.len())
                    .ok_or(ProtocolError::InvalidPayload("display item edit range"))?;
                items.extend_from_slice(&base.items[copied..start]);
                if items.len() + inserted > limit {
                    return Err(ProtocolError::InvalidPayload("display items"));
                }
                for _ in 0..inserted {
                    items.push(decode_item(reader)?);
                }
                copied = end;
            }
            items.extend_from_slice(&base.items[copied..]);
            if items.len() > limit {
                return Err(ProtocolError::InvalidPayload("display items"));
            }
            Ok(items)
        }
        _ => Err(ProtocolError::InvalidPayload("display item encoding")),
    }
}

/// Edits that turn `old` into `new`, or `None` when they would replace most of it. Keeps the
/// common prefix and suffix; between them, lists of equal length (a recolor, a moved box) are
/// compared item by item, anything else is one replacement.
fn edits(old: &[DisplayItem], new: &[DisplayItem]) -> Option<Vec<Edit>> {
    let prefix = old
        .iter()
        .zip(new)
        .take_while(|(a, b)| same_display_item(a, b))
        .count();
    let suffix = old[prefix..]
        .iter()
        .rev()
        .zip(new[prefix..].iter().rev())
        .take_while(|(a, b)| same_display_item(a, b))
        .count();
    let (old_middle, new_middle) = (
        &old[prefix..old.len() - suffix],
        &new[prefix..new.len() - suffix],
    );
    // Sending more than half of the new list costs about as much as sending all of it.
    let budget = new.len() / 2;
    if old_middle.len() != new_middle.len() {
        return (new_middle.len() <= budget).then(|| {
            vec![Edit {
                start: prefix,
                removed: old_middle.len(),
                inserted: new_middle.len(),
            }]
        });
    }
    let mut edits = Vec::new();
    let mut sent = 0;
    let mut index = 0;
    while index < old_middle.len() {
        if same_display_item(&old_middle[index], &new_middle[index]) {
            index += 1;
            continue;
        }
        let start = index;
        while index < old_middle.len() && !same_display_item(&old_middle[index], &new_middle[index])
        {
            index += 1;
        }
        sent += index - start;
        if sent > budget {
            return None;
        }
        edits.push(Edit {
            start: prefix + start,
            removed: index - start,
            inserted: index - start,
        });
    }
    Some(edits)
}

#[cfg(test)]
mod tests;
