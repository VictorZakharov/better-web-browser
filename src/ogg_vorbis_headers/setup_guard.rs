//! Reject pathological codebook allocations before entering Lewton's setup parser.
//! This reads only the codebook prefix of the third Vorbis header, following the
//! Xiph Vorbis I specification's bitpacking and codebook configuration sections.

const MAX_BOOK_ENTRIES: u32 = 65_536;
const MAX_ALL_ENTRIES: u64 = 262_144;
const MAX_BOOK_DIMENSIONS: u32 = 256;
const MAX_ALL_VQ_VALUES: u64 = 2_000_000;

pub(super) fn inspect(packet: &[u8]) -> Result<(), String> {
    let mut bits = Bits::new(packet.get(7..).ok_or("Vorbis setup header is truncated")?);
    let book_count = bits.read(8)? + 1;
    let mut total_entries = 0_u64;
    let mut total_vectors = 0_u64;
    for _ in 0..book_count {
        if bits.read(24)? != 0x56_43_42 {
            return Err("Vorbis setup codebook sync is invalid".into());
        }
        let dimensions = bits.read(16)?;
        let entries = bits.read(24)?;
        total_entries += u64::from(entries);
        if dimensions == 0
            || dimensions > MAX_BOOK_DIMENSIONS
            || entries == 0
            || entries > MAX_BOOK_ENTRIES
            || total_entries > MAX_ALL_ENTRIES
        {
            return Err("Vorbis setup codebook exceeds worker allocation limit".into());
        }
        if bits.read(1)? == 0 {
            let sparse = bits.read(1)? != 0;
            if sparse {
                for _ in 0..entries {
                    if bits.read(1)? != 0 {
                        bits.skip(5)?;
                    }
                }
            } else {
                bits.skip(entries as usize * 5)?;
            }
        } else {
            bits.skip(5)?;
            let mut described = 0_u32;
            // Ordered lengths cannot use more than 32 distinct Huffman lengths.
            for _ in 0..32 {
                if described == entries {
                    break;
                }
                let remaining = entries - described;
                let width = (32 - remaining.leading_zeros()) as usize;
                let count = bits.read(width)?;
                if count > remaining {
                    return Err("Vorbis setup codeword count exceeds entries".into());
                }
                described += count;
            }
            if described != entries {
                return Err("Vorbis setup ordered codewords are incomplete".into());
            }
        }
        let lookup = bits.read(4)?;
        if lookup > 2 {
            return Err("Vorbis setup lookup type is unsupported".into());
        }
        if lookup != 0 {
            bits.skip(64)?; // minimum and delta values
            let value_bits = bits.read(4)? + 1;
            bits.skip(1)?; // sequence flag
            let encoded_values = if lookup == 1 {
                lookup1_values(entries, dimensions)
            } else {
                u64::from(entries) * u64::from(dimensions)
            };
            let expanded_values = u64::from(entries) * u64::from(dimensions);
            total_vectors = total_vectors
                .checked_add(expanded_values)
                .ok_or("Vorbis setup vector count overflow")?;
            if total_vectors > MAX_ALL_VQ_VALUES {
                return Err("Vorbis setup vectors exceed worker allocation limit".into());
            }
            bits.skip(
                usize::try_from(encoded_values * u64::from(value_bits))
                    .map_err(|_| "Vorbis setup lookup length overflow")?,
            )?;
        }
    }
    Ok(())
}

fn lookup1_values(entries: u32, dimensions: u32) -> u64 {
    let mut lower = 1_u32;
    let mut upper = entries;
    while lower < upper {
        let candidate = lower + (upper - lower).div_ceil(2);
        let mut power = 1_u64;
        for _ in 0..dimensions {
            power = power.saturating_mul(u64::from(candidate));
            if power > u64::from(entries) {
                break;
            }
        }
        if power <= u64::from(entries) {
            lower = candidate;
        } else {
            upper = candidate - 1;
        }
    }
    u64::from(lower)
}

struct Bits<'a> {
    source: &'a [u8],
    offset: usize,
}

impl<'a> Bits<'a> {
    fn new(source: &'a [u8]) -> Self {
        Self { source, offset: 0 }
    }

    fn read(&mut self, count: usize) -> Result<u32, String> {
        if count > 32 {
            return Err("Vorbis setup field is too wide".into());
        }
        let next = self.bounded_offset(count)?;
        let mut value = 0_u32;
        for bit in 0..count {
            let position = self.offset + bit;
            let set = (self.source[position / 8] >> (position % 8)) & 1;
            value |= u32::from(set) << bit;
        }
        self.offset = next;
        Ok(value)
    }

    fn skip(&mut self, count: usize) -> Result<(), String> {
        self.offset = self.bounded_offset(count)?;
        Ok(())
    }

    fn bounded_offset(&self, count: usize) -> Result<usize, String> {
        self.offset
            .checked_add(count)
            .filter(|end| *end <= self.source.len() * 8)
            .ok_or_else(|| "Vorbis setup header is truncated".into())
    }
}
