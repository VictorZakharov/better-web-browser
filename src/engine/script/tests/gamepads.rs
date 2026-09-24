use super::*;

#[test]
fn gamepad_interface_is_polled_but_uninteracted_devices_remain_private() {
    let (_, outcome) = execute_html(
        r#"<script>
        if (typeof navigator.getGamepads !== 'function' ||
            typeof Gamepad !== 'function' || typeof GamepadButton !== 'function' ||
            typeof GamepadEvent !== 'function') throw Error('missing interfaces');
        const first = navigator.getGamepads(), second = navigator.getGamepads();
        if (!Array.isArray(first) || first.length !== 4 || second.length !== 4)
            throw Error('snapshot shape');
        if (first.some(pad => pad && (!(pad instanceof Gamepad) || pad.mapping !== 'standard' ||
            pad.buttons.length !== 17 || pad.axes.length !== 4))) throw Error('bad mapping');
        try { new Gamepad(); throw Error('public Gamepad constructor'); }
        catch (error) { if (!(error instanceof TypeError)) throw error; }
        try { new GamepadButton(); throw Error('public GamepadButton constructor'); }
        catch (error) { if (!(error instanceof TypeError)) throw error; }
        try { new GamepadEvent('gamepadconnected', {gamepad: {}}); throw Error('bad gamepad accepted'); }
        catch (error) { if (!(error instanceof TypeError)) throw error; }
        try { navigator.getGamepads.call({}); throw Error('bad receiver accepted'); }
        catch (error) { if (!(error instanceof TypeError)) throw error; }
        console.log('gamepad interfaces passed');
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.console, ["log: gamepad interfaces passed"]);
}
