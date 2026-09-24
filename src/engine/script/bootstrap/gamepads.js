    // The standardized mapping is XInput's button order, not its bit order.
    // A controller remains hidden until the user actually manipulates it.
    // https://www.w3.org/TR/gamepad/#getgamepads-method
    if (host('gamepadAvailable')) {
        const gamepadStates = new WeakMap();
        const buttonStates = new WeakMap();
        const buttonMasks = [0x1000, 0x2000, 0x4000, 0x8000, 0x0100, 0x0200,
            null, null, 0x0020, 0x0010, 0x0040, 0x0080, 0x0001, 0x0002,
            0x0004, 0x0008, null];
        const slots = [null, null, null, null], exposed = [false, false, false, false];
        let polling = false;
        function Gamepad() { throw new TypeError('Illegal constructor'); }
        function GamepadButton() { throw new TypeError('Illegal constructor'); }
        // Event is an ES class in this engine; subclass it for proper dispatch.
        class GamepadConnectionEvent extends Event {
            constructor(type, init = {}) {
                if (arguments.length < 1) throw new TypeError('GamepadEvent requires a type');
                super(type, init);
                if (!(init.gamepad instanceof Gamepad))
                    throw new TypeError('GamepadEvent requires a Gamepad');
                Object.defineProperty(this, 'gamepad', {enumerable: true, value: init.gamepad});
            }
        }
        Object.defineProperty(GamepadConnectionEvent.prototype, Symbol.toStringTag,
            {value: 'GamepadEvent', configurable: true});
        Object.defineProperties(Gamepad.prototype, {
            [Symbol.toStringTag]: {value: 'Gamepad', configurable: true},
            id: {enumerable: true, get() { return gamepadStates.get(this).id; }},
            index: {enumerable: true, get() { return gamepadStates.get(this).index; }},
            connected: {enumerable: true, get() { return gamepadStates.get(this).connected; }},
            timestamp: {enumerable: true, get() { return gamepadStates.get(this).timestamp; }},
            mapping: {enumerable: true, get() {
                if (!gamepadStates.has(this)) throw new TypeError('Invalid Gamepad receiver');
                return 'standard';
            }},
            axes: {enumerable: true, get() { return gamepadStates.get(this).axes; }},
            buttons: {enumerable: true, get() { return gamepadStates.get(this).buttons; }},
            vibrationActuator: {enumerable: true, get() { return null; }}
        });
        for (const name of ['pressed', 'touched', 'value'])
            Object.defineProperty(GamepadButton.prototype, name, {enumerable: true,
                get() {
                    const state = buttonStates.get(this);
                    if (!state) throw new TypeError('Invalid GamepadButton receiver');
                    return state[name];
                }});
        Object.defineProperty(GamepadButton.prototype, Symbol.toStringTag,
            {value: 'GamepadButton', configurable: true});
        const makeButton = () => {
            const button = Object.create(GamepadButton.prototype);
            buttonStates.set(button, {pressed: false, touched: false, value: 0});
            return button;
        };
        const makePad = index => {
            const pad = Object.create(Gamepad.prototype);
            gamepadStates.set(pad, {id: 'XInput Controller (STANDARD GAMEPAD)', index,
                connected: true, timestamp: 0, axes: [0, 0, 0, 0],
                buttons: Array.from({length: 17}, makeButton), packet: -1});
            return pad;
        };
        const axis = (value, negative) => Math.max(-1, Math.min(1,
            (negative ? -value : value) / (value < 0 ? 32768 : 32767)));
        const interacted = raw => raw[1] !== 0 || raw[2] > 30 || raw[3] > 30 ||
            Math.abs(raw[4]) > 7849 || Math.abs(raw[5]) > 7849 ||
            Math.abs(raw[6]) > 8689 || Math.abs(raw[7]) > 8689;
        const update = (pad, raw) => {
            const state = gamepadStates.get(pad);
            if (state.packet === raw[0]) return;
            state.packet = raw[0];
            state.timestamp = performance.now();
            const [packet, mask, lt, rt, lx, ly, rx, ry] = raw;
            state.axes[0] = axis(lx, false); state.axes[1] = axis(ly, true);
            state.axes[2] = axis(rx, false); state.axes[3] = axis(ry, true);
            for (let i = 0; i < buttonMasks.length; i++) {
                const value = i === 6 ? lt / 255 : i === 7 ? rt / 255 :
                    i === 16 ? 0 : (mask & buttonMasks[i]) ? 1 : 0;
                const button = buttonStates.get(state.buttons[i]);
                button.value = value;
                button.pressed = value > (i === 6 || i === 7 ? 0.5 : 0);
                button.touched = value > 0;
            }
        };
        const poll = () => {
            const rawSlots = host('gamepadPoll');
            for (let index = 0; index < slots.length; index++) {
                const raw = rawSlots[index], old = slots[index];
                if (!raw) {
                    if (old) {
                        gamepadStates.get(old).connected = false;
                        slots[index] = null; exposed[index] = false;
                        windowObject.dispatchEvent(markTrusted(new GamepadConnectionEvent(
                            'gamepaddisconnected', {gamepad: old})));
                    }
                    continue;
                }
                if (!exposed[index] && !interacted(raw)) continue;
                if (!old) {
                    const pad = slots[index] = makePad(index);
                    exposed[index] = true;
                    update(pad, raw);
                    windowObject.dispatchEvent(markTrusted(new GamepadConnectionEvent(
                        'gamepadconnected', {gamepad: pad})));
                } else update(old, raw);
            }
        };
        const continuePolling = () => {
            poll();
            windowObject.setTimeout(continuePolling, slots.some(Boolean) ? 100 : 500);
        };
        windowObject.navigator.getGamepads = function getGamepads() {
            if (this !== windowObject.navigator) throw new TypeError('Invalid Navigator receiver');
            poll();
            if (!polling) { polling = true; windowObject.setTimeout(continuePolling, 100); }
            return slots.slice();
        };
        Object.assign(windowObject, {Gamepad, GamepadButton, GamepadEvent: GamepadConnectionEvent});
    }
