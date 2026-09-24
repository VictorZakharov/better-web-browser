//! A bounded XInput snapshot for the Gamepad API. The JS layer applies the
//! specification's interaction gate before any device is visible to a page.
//! https://www.w3.org/TR/gamepad/#dfn-gamepad-user-gesture

use super::value::JsValue;
use windows_sys::Win32::UI::Input::XboxController::{
    XINPUT_STATE, XInputGetState, XUSER_MAX_COUNT,
};

const MAX_GAMEPADS: u32 = 4;

pub(super) fn poll() -> JsValue {
    JsValue::Array(
        (0..MAX_GAMEPADS.min(XUSER_MAX_COUNT))
            .map(snapshot)
            .collect(),
    )
}

fn snapshot(index: u32) -> JsValue {
    let mut state = XINPUT_STATE::default();
    if unsafe { XInputGetState(index, &mut state) } != 0 {
        return JsValue::Null;
    }
    raw_state(&state)
}

fn raw_state(state: &XINPUT_STATE) -> JsValue {
    let pad = &state.Gamepad;
    JsValue::Array(
        [
            state.dwPacketNumber as f64,
            pad.wButtons as f64,
            pad.bLeftTrigger as f64,
            pad.bRightTrigger as f64,
            pad.sThumbLX as f64,
            pad.sThumbLY as f64,
            pad.sThumbRX as f64,
            pad.sThumbRY as f64,
        ]
        .into_iter()
        .map(JsValue::Number)
        .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows_sys::Win32::UI::Input::XboxController::XINPUT_GAMEPAD_A;

    #[test]
    fn xinput_snapshot_preserves_buttons_triggers_and_signed_axes() {
        let mut state = XINPUT_STATE {
            dwPacketNumber: 42,
            ..Default::default()
        };
        state.Gamepad.wButtons = XINPUT_GAMEPAD_A;
        state.Gamepad.bLeftTrigger = 31;
        state.Gamepad.sThumbLX = -1234;
        assert_eq!(
            raw_state(&state),
            JsValue::Array(vec![
                JsValue::Number(42.0),
                JsValue::Number(4096.0),
                JsValue::Number(31.0),
                JsValue::Number(0.0),
                JsValue::Number(-1234.0),
                JsValue::Number(0.0),
                JsValue::Number(0.0),
                JsValue::Number(0.0),
            ])
        );
    }
}
