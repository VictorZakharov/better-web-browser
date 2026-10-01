//! WebM family-0 Opus: shared upstream demux/codec, bounded presentation policy.
//! The Matroska CodecDelay and DiscardPadding metadata cannot be inferred from
//! Symphonia's packet duration (which is often zero and omits discard padding).

mod elements;
pub(crate) mod header;
mod metadata;
pub(crate) mod mux;
mod packets;
mod stream;
mod timeline;
pub(crate) use stream::Stream;

pub(crate) fn sniff(bytes: &[u8]) -> bool {
    // Only inspect CodecID at its schema path; a comment containing A_OPUS must
    // not select the Opus backend. Malformed WebM still fails in its own family.
    if bytes.len() > crate::limits::MAX_MEDIA_ENCODED_QUEUE_BYTES
        || !bytes.starts_with(&[0x1a, 0x45, 0xdf, 0xa3])
    {
        return false;
    }
    metadata::codec_id(bytes).is_ok_and(|codec| codec == b"A_OPUS")
}

#[cfg(test)]
mod tests;
