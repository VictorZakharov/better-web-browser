//! A bounded, synchronous read-only bitmap predicate. Unlike a painter it
//! returns no storage, needs no copied allocation and retains no author handle.
use super::*;

pub(super) fn dispatch(
    scope: &mut v8::PinScope,
    arguments: v8::FunctionCallbackArguments,
    mut output: v8::ReturnValue,
) {
    let Some(bridge) = scope.get_current_context().get_slot::<HostBridge>() else {
        throw_error(scope, inactive_host());
        return;
    };
    let active = match &*bridge {
        HostBridge::Document(host) => host.upgrade().is_some(),
        HostBridge::Worker(host) => host.upgrade().is_some(),
    };
    if !active {
        throw_error(scope, inactive_host());
        return;
    }
    let started = bridge.profile_start();
    let result = inspect(scope, &arguments);
    bridge.profile_bridge("canvasBitmapHasHiddenRgb", started);
    match result {
        Some(hidden) => output.set(v8::Boolean::new(scope, hidden).into()),
        None => output.set(v8::null(scope).into()),
    }
}

fn inspect(scope: &mut v8::PinScope, args: &v8::FunctionCallbackArguments) -> Option<bool> {
    let value = args.get(1);
    if args.length() != 2 || (!value.is_uint8_array() && !value.is_uint8_clamped_array()) {
        return None;
    }
    let view = v8::Local::<v8::ArrayBufferView>::try_from(value).ok()?;
    let buffer = view.buffer(scope)?;
    let storage = buffer.get_backing_store();
    if buffer.is_shared_array_buffer()
        || buffer.was_detached()
        || storage.is_resizable_by_user_javascript()
    {
        return None;
    }
    let offset = view.byte_offset();
    let length = view.byte_length();
    if !length.is_multiple_of(4)
        || length > crate::limits::MAX_CANVAS_PIXELS * 4
        || offset.checked_add(length)? > storage.byte_length()
    {
        return None;
    }
    if length == 0 {
        return Some(false);
    }
    let pointer = storage.data()?.as_ptr().cast::<u8>();
    // SAFETY: The retained backing store pins the admitted in-bounds span.
    // Shared/growable/resizable/detached buffers are rejected. This pure Rust
    // predicate calls no V8/author code and neither mutates nor retains bytes;
    // the calling isolate cannot repaint or detach its fixed storage meanwhile.
    let bytes = unsafe { std::slice::from_raw_parts(pointer.add(offset), length) };
    Some(hidden_rgb(bytes))
}

fn hidden_rgb(bytes: &[u8]) -> bool {
    #[cfg(target_arch = "x86_64")]
    {
        // x86-64 guarantees SSE2. Each lane checks alpha==0 && RGB!=0;
        // unaligned byte views are legal and the tail stays scalar.
        use std::arch::x86_64::*;
        let mut chunks = bytes.chunks_exact(16);
        // SAFETY: SSE2 is baseline, loads cover exactly one complete chunk and
        // no vector pointer or reference escapes this synchronous predicate.
        let found = unsafe {
            let alpha = _mm_set1_epi32(0xff000000u32 as i32);
            let rgb = _mm_set1_epi32(0x00ffffff);
            let zero = _mm_setzero_si128();
            chunks.any(|chunk| {
                let pixels = _mm_loadu_si128(chunk.as_ptr().cast());
                let transparent = _mm_cmpeq_epi32(_mm_and_si128(pixels, alpha), zero);
                let black = _mm_cmpeq_epi32(_mm_and_si128(pixels, rgb), zero);
                _mm_movemask_epi8(_mm_andnot_si128(black, transparent)) != 0
            })
        };
        found || scalar(chunks.remainder())
    }
    #[cfg(not(target_arch = "x86_64"))]
    scalar(bytes)
}

fn scalar(bytes: &[u8]) -> bool {
    bytes
        .chunks_exact(4)
        .any(|p| p[3] == 0 && p[..3] != [0, 0, 0])
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn vector_predicate_matches_scalar_for_offsets_alpha_channels_and_tails() {
        for offset in 0..16 {
            for length in 0..35 {
                for channel in 0..4 {
                    let mut backing = vec![0; offset + length * 4];
                    if length > 0 {
                        backing[offset + (length / 2) * 4 + channel] = 1;
                    }
                    let bytes = &backing[offset..];
                    assert_eq!(
                        hidden_rgb(bytes),
                        scalar(bytes),
                        "{offset}/{length}/{channel}"
                    );
                }
            }
        }
        assert!(!hidden_rgb(&[255, 0, 0, 1].repeat(64)));
        assert!(hidden_rgb(&[255, 0, 0, 0].repeat(64)));
    }
}
