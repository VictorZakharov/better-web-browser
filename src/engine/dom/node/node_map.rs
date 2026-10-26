//! Maps keyed by [`NodeId`] with a hasher suited to it.
//
// Style and layout look nodes up in `NodeId`-keyed maps several times per node and pass. The
// standard SipHash guards against keys chosen to collide, but node identifiers are allocator
// sequence numbers no page controls, and `NodeId` hashes a single word. One folded multiply
// spreads that word over the bits a table indexes by, at a fraction of SipHash's cost.
#[cfg(doc)]
use super::NodeId;
use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hasher};

pub type NodeMap<K, V> = HashMap<K, V, BuildHasherDefault<NodeHasher>>;

/// For [`NodeId`] keys, alone or combined with other small keys.
#[derive(Default)]
pub struct NodeHasher(u64);

const SEED: u64 = 0x51_7c_c1_b7_27_22_0a_95;

impl Hasher for NodeHasher {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.write_u64(u64::from(byte));
        }
    }

    /// A folded multiply (as in foldhash and aHash): the 128-bit product's halves combined make
    /// every output bit depend on every input bit. A plain 64-bit product's low bits, which
    /// tables index by, would ignore the high bits where identifiers of different documents
    /// differ.
    fn write_u64(&mut self, value: u64) {
        let product = u128::from(self.0 ^ value) * u128::from(SEED);
        self.0 = (product as u64) ^ ((product >> 64) as u64);
    }

    fn write_u32(&mut self, value: u32) {
        self.write_u64(u64::from(value));
    }

    fn write_u8(&mut self, value: u8) {
        self.write_u64(u64::from(value));
    }

    fn write_usize(&mut self, value: usize) {
        self.write_u64(value as u64);
    }

    fn write_isize(&mut self, value: isize) {
        self.write_u64(value as u64);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::dom::{Node, NodeId};

    #[test]
    fn distinct_nodes_spread_over_table_buckets() {
        let ids = (0..4096)
            .map(|_| Node::create_document().id())
            .collect::<Vec<NodeId>>();
        let mut map = NodeMap::default();
        for (index, id) in ids.iter().enumerate() {
            map.insert(*id, index);
        }
        assert!(ids.iter().enumerate().all(|(index, id)| map[id] == index));
        let document = Node::create_document();
        let elements = (0..4096)
            .map(|_| Node::create_element_for(&document, "p").id())
            .collect::<Vec<NodeId>>();
        // The low bits a table indexes by differ for sequential identifiers, within one
        // document and across documents.
        for ids in [ids, elements] {
            let buckets = ids
                .iter()
                .map(|id| {
                    let mut hasher = NodeHasher::default();
                    std::hash::Hash::hash(id, &mut hasher);
                    hasher.finish() & 1023
                })
                .collect::<std::collections::HashSet<_>>();
            assert!(buckets.len() > 900, "{}", buckets.len());
        }
    }
}
