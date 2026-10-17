//! Unsent same-direction wheel inputs merge instead of exhausting the bounded queue.
//
// Wheel events need not map one-to-one to device notches. While the renderer is
// busy, one cancelable event carries the combined delta at the newest pointer
// position, as Chromium's queue does for compatible WebMouseWheelEvents. Opposite
// directions stay separate so a reversal remains a native-scrolling fence.
use better_web_browser::renderer_protocol::{DocumentInput, PointerPhase, WheelInput};
use std::collections::VecDeque;

// Matches the renderer protocol's validated input bound.
const MAX_WHEEL_DELTA: f32 = 16_777_216.0;

/// Returns the index of an unsent wheel that `newer` can absorb. Only absolute
/// pointer moves and viewport positions may separate them: both are replaceable
/// state, so delivering the merged wheel after them preserves every discrete barrier.
pub(super) fn coalescible_index(
    inputs: &VecDeque<DocumentInput>,
    newer: &WheelInput,
    floor: u64,
) -> Option<usize> {
    for (index, pending) in inputs.iter().enumerate().rev() {
        match pending {
            DocumentInput::Wheel(older) => {
                return compatible(older, newer, floor).then_some(index);
            }
            DocumentInput::Pointer(pointer) if pointer.phase == PointerPhase::Move => {}
            DocumentInput::Scroll(_) => {}
            _ => return None,
        }
    }
    None
}

pub(super) fn merge(older: &WheelInput, newer: &WheelInput) -> WheelInput {
    WheelInput {
        delta_x: combined(older.delta_x, newer.delta_x),
        delta_y: combined(older.delta_y, newer.delta_y),
        ..*newer
    }
}

fn compatible(older: &WheelInput, newer: &WheelInput, floor: u64) -> bool {
    older.document == newer.document
        && older.sequence >= floor
        && older.sequence < newer.sequence
        && older.modifiers == newer.modifiers
        && older.target == newer.target
        && same_direction(older.delta_x, newer.delta_x)
        && same_direction(older.delta_y, newer.delta_y)
}

fn same_direction(older: f32, newer: f32) -> bool {
    (older > 0.0) == (newer > 0.0) && (older < 0.0) == (newer < 0.0)
}

fn combined(older: f32, newer: f32) -> f32 {
    (older + newer).clamp(-MAX_WHEEL_DELTA, MAX_WHEEL_DELTA)
}
