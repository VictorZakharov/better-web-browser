//! Deterministic shaping fixtures derived from the bundled CC0 Ahem font.
//! Append a standard GSUB ligature and kern pair, not synthetic browser results.
//! Ahem provenance remains in tests/canvas/fonts/README.md.

pub(crate) fn font_bytes() -> Vec<u8> {
    let original = include_bytes!("../../../tests/canvas/fonts/ahem.ttf");
    let font = swash::FontRef::from_index(original, 0).unwrap();
    let glyph = |c| font.charmap().map(c);
    let gsub = substitution_table(glyph('f'), glyph('i'), glyph('1'), glyph('C'));
    let kern = kerning_table(glyph('A'), glyph('V'));
    append_tables(original, vec![(*b"GSUB", gsub), (*b"kern", kern)])
}

fn words(values: &[u16]) -> Vec<u8> {
    values
        .iter()
        .flat_map(|value| value.to_be_bytes())
        .collect()
}

fn substitution_table(first: u16, second: u16, digit: u16, replacement: u16) -> Vec<u8> {
    let mut script = words(&[1]);
    script.extend_from_slice(b"latn");
    script.extend(words(&[8, 4, 0, 0, 0xffff, 2, 0, 1]));
    let mut feature = words(&[2]);
    feature.extend_from_slice(b"liga");
    feature.extend(words(&[14]));
    feature.extend_from_slice(b"onum");
    feature.extend(words(&[20, 0, 1, 0, 0, 1, 1]));
    let ligature = words(&[
        4,
        0,
        1,
        8, // Ligature-substitution lookup, one subtable.
        1,
        18,
        1,
        8, // Format 1, coverage at 18, one LigatureSet at 8.
        1,
        4, // One Ligature record at offset 4 inside its set.
        replacement,
        2,
        second, // Substitute two glyphs with the real C outline.
        1,
        1,
        first, // CoverageFormat1 containing the real f glyph.
    ]);
    let numeric = words(&[
        1,
        0,
        1,
        8, // Single-substitution lookup, one subtable at 8.
        2,
        8,
        1,
        replacement, // Format 2, one glyph, coverage at 8.
        1,
        1,
        digit, // Replace the real digit with the real C outline.
    ]);
    let mut lookup = words(&[2, 6, (6 + ligature.len()) as u16]);
    lookup.extend(ligature);
    lookup.extend(numeric);
    let mut table = words(&[
        1,
        0,
        10,
        (10 + script.len()) as u16,
        (10 + script.len() + feature.len()) as u16,
    ]);
    table.extend(script);
    table.extend(feature);
    table.extend(lookup);
    table
}

fn kerning_table(left: u16, right: u16) -> Vec<u8> {
    words(&[
        0,
        1, // Version 0, one subtable.
        0,
        20,
        1, // Version 0, 20-byte subtable, horizontal format 0.
        1,
        6,
        0,
        0, // One pair and its binary-search header.
        left,
        right,
        (-200_i16) as u16,
    ])
}

fn append_tables(original: &[u8], extra: Vec<([u8; 4], Vec<u8>)>) -> Vec<u8> {
    let read16 = |offset| u16::from_be_bytes(original[offset..offset + 2].try_into().unwrap());
    let read32 =
        |offset| u32::from_be_bytes(original[offset..offset + 4].try_into().unwrap()) as usize;
    let mut tables: Vec<([u8; 4], Vec<u8>)> = (0..usize::from(read16(4)))
        .map(|index| {
            let record = 12 + index * 16;
            let tag = original[record..record + 4].try_into().unwrap();
            let start = read32(record + 8);
            let len = read32(record + 12);
            (tag, original[start..start + len].to_vec())
        })
        .filter(|(tag, _)| !extra.iter().any(|(new, _)| tag == new))
        .collect();
    tables.extend(extra);
    tables.sort_by_key(|(tag, _)| *tag);
    let count = tables.len() as u16;
    let selector = (u16::BITS - 1 - count.leading_zeros()) as u16;
    let search = (1_u16 << selector) * 16;
    let mut out = vec![0_u8; 12 + tables.len() * 16];
    out[..4].copy_from_slice(&original[..4]);
    out[4..12].copy_from_slice(&words(&[count, search, selector, count * 16 - search]));
    let mut head = None;
    for (index, (tag, mut data)) in tables.into_iter().enumerate() {
        if tag == *b"head" {
            data[8..12].fill(0);
            head = Some(out.len());
        }
        let offset = out.len() as u32;
        let record = 12 + index * 16;
        out[record..record + 4].copy_from_slice(&tag);
        out[record + 4..record + 8].copy_from_slice(&checksum(&data).to_be_bytes());
        out[record + 8..record + 12].copy_from_slice(&offset.to_be_bytes());
        out[record + 12..record + 16].copy_from_slice(&(data.len() as u32).to_be_bytes());
        out.extend(data);
        while !out.len().is_multiple_of(4) {
            out.push(0);
        }
    }
    let adjustment = 0xB1B0_AFBA_u32.wrapping_sub(checksum(&out));
    let head = head.unwrap();
    out[head + 8..head + 12].copy_from_slice(&adjustment.to_be_bytes());
    out
}

fn checksum(bytes: &[u8]) -> u32 {
    bytes.chunks(4).fold(0_u32, |sum, chunk| {
        let mut word = [0_u8; 4];
        word[..chunk.len()].copy_from_slice(chunk);
        sum.wrapping_add(u32::from_be_bytes(word))
    })
}

#[test]
fn fixture_directory_and_global_sfnt_checksum_are_structurally_valid() {
    let bytes = font_bytes();
    assert_eq!(checksum(&bytes), 0xB1B0_AFBA);
    assert!(harfrust::FontRef::from_index(&bytes, 0).is_ok());
}
