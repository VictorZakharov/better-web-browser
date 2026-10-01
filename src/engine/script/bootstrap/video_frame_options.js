    // WebCodecs frame storage is private. Author properties cannot replace coded
    // pixels, close state or color metadata. Numeric conversion follows Web IDL.
    const videoFrameStates = new WeakMap();
    const videoColorStates = new WeakMap();
    const frameError = (message, name = 'InvalidStateError') => new DOMException(message, name);
    const frameDictionary = value => {
        if (value === undefined || value === null) return {};
        if (typeof value !== 'object' && typeof value !== 'function')
            throw new TypeError('Frame options must be a dictionary');
        return value;
    };
    const frameString = value => {
        if (typeof value === 'symbol') throw new TypeError('Cannot convert Symbol to DOMString');
        return String(value);
    };
    const frameRange = (value, minimum, maximum) => {
        const number = bitmapNumber(value);
        if (!Number.isFinite(number)) throw new TypeError('Frame integer must be finite');
        const integer = Math.trunc(number);
        if (integer < minimum || integer > maximum)
            throw new TypeError('Frame integer is outside its allowed range');
        return integer === 0 ? 0 : integer;
    };
    const frameUint = value => frameRange(value, 0, 0xffffffff);
    const frameTime = (value, unsigned = false) => {
        const number = bitmapNumber(value);
        if (!Number.isFinite(number)) throw new TypeError('Frame time must be finite');
        const integer = BigInt(Math.trunc(number));
        const minimum = unsigned ? 0n : -(1n << 63n);
        const maximum = unsigned ? (1n << 64n) - 1n : (1n << 63n) - 1n;
        if (integer < minimum || integer > maximum) throw new TypeError('Frame time is out of range');
        return Number(integer);
    };
    const frameLongTime = (value, unsigned = false) => {
        const number = bitmapNumber(value);
        if (!Number.isFinite(number) || number === 0) return 0;
        const integer = BigInt(Math.trunc(number));
        return Number(unsigned ? BigInt.asUintN(64, integer) : BigInt.asIntN(64, integer));
    };
    const frameRequired = (value, name) => {
        if (value === undefined) throw new TypeError('Missing required frame member: ' + name);
        return value;
    };
    const frameEnum = (value, values, fallback) => {
        if (value === undefined) return fallback;
        const result = frameString(value);
        if (!values.includes(result)) throw new TypeError('Invalid frame enum: ' + result);
        return result;
    };
    const frameBuffer = value => {
        try {
            if (value instanceof ArrayBuffer ||
                (typeof SharedArrayBuffer === 'function' && value instanceof SharedArrayBuffer))
                return new Uint8Array(value);
            if (ArrayBuffer.isView(value))
                return new Uint8Array(value.buffer, value.byteOffset, value.byteLength);
        } catch (_) { throw new TypeError('Frame buffer is detached'); }
        throw new TypeError('Frame pixels require a BufferSource');
    };
    const frameTransfers = value => {
        if (value === undefined) return [];
        if (value === null || typeof value[Symbol.iterator] !== 'function')
            throw new TypeError('Frame transfer must be a sequence');
        const transfers = [...value];
        for (const buffer of transfers) {
            if (!(buffer instanceof ArrayBuffer)) throw new TypeError('Only ArrayBuffers may be transferred');
        }
        return transfers;
    };
    const frameValidateTransfers = transfers => {
        const seen = new Set();
        for (const buffer of transfers) {
            if (seen.has(buffer)) throw frameError('Duplicate transfer buffer', 'DataCloneError');
            try { new Uint8Array(buffer); }
            catch (_) { throw frameError('Transfer buffer is detached', 'DataCloneError'); }
            seen.add(buffer);
        }
    };
    const frameRectOptions = value => {
        if (value === undefined) return undefined;
        const options = frameDictionary(value);
        // DOMRectInit visits height, width, x and y in dictionary order.
        const number = (value, fallback) => value === undefined ? fallback : bitmapNumber(value);
        const height = number(options.height, 0), width = number(options.width, 0);
        const x = number(options.x, 0), y = number(options.y, 0);
        return { x, y, width, height };
    };
    const frameRect = (value, width, height) => {
        const rect = value ?? { x: 0, y: 0, width, height };
        for (const member of ['x', 'y', 'width', 'height']) {
            if (!Number.isFinite(rect[member]) || rect[member] < 0)
                throw new TypeError('Frame rectangle must be finite and nonnegative');
        }
        const result = {x: Math.trunc(rect.x), y: Math.trunc(rect.y),
            width: Math.trunc(rect.width), height: Math.trunc(rect.height)};
        if (!result.width || !result.height || rect.x + rect.width > width ||
            rect.y + rect.height > height)
            throw new TypeError('Frame rectangle is empty or outside the coded image');
        return result;
    };
    const frameRotation = value => {
        const rotation = value === undefined ? 0 : bitmapNumber(value);
        if (!Number.isFinite(rotation)) throw new TypeError('Frame rotation must be finite');
        // WebCodecs rounds to the closest clockwise quarter-turn.
        return ((Math.floor(rotation / 90 + 0.5) * 90) % 360 + 360) % 360;
    };
    const frameLayoutOptions = value => {
        if (value === undefined) return undefined;
        if (value === null || typeof value[Symbol.iterator] !== 'function')
            throw new TypeError('Frame layout must be a sequence');
        return Array.from(value, item => {
            const options = frameDictionary(item);
            const offset = frameUint(frameRequired(options.offset, 'offset'));
            const stride = frameUint(frameRequired(options.stride, 'stride'));
            return {offset, stride};
        });
    };
    const frameFormats = ['I420', 'I420A', 'I422', 'I444', 'NV12',
        'RGBA', 'RGBX', 'BGRA', 'BGRX', 'I420P10', 'I420P12', 'I422P10',
        'I422P12', 'I444P10', 'I444P12', 'I420AP10', 'I420AP12',
        'I422A', 'I422AP10', 'I422AP12', 'I444A', 'I444AP10', 'I444AP12', 'RGBAF16'];
    const frameColorOptions = value => {
        const options = frameDictionary(value);
        const nullableEnum = (value, values) => value === null ? null : frameEnum(value, values, null);
        const rawRange = options.fullRange;
        const fullRange = rawRange === undefined || rawRange === null ? null : Boolean(rawRange);
        const matrix = nullableEnum(options.matrix, ['rgb', 'bt709', 'bt470bg', 'smpte170m', 'bt2020-ncl']);
        const primaries = nullableEnum(options.primaries, ['bt709', 'bt470bg', 'smpte170m', 'bt2020', 'smpte432']);
        const transfer = nullableEnum(options.transfer, ['bt709', 'smpte170m', 'iec61966-2-1', 'linear', 'pq', 'hlg']);
        return {
            fullRange, matrix, primaries, transfer
        };
    };
    class VideoColorSpace {
        constructor(init = {}) { videoColorStates.set(this, frameColorOptions(init)); }
        get primaries() { return videoColorState(this).primaries; }
        get transfer() { return videoColorState(this).transfer; }
        get matrix() { return videoColorState(this).matrix; }
        get fullRange() { return videoColorState(this).fullRange; }
        toJSON() { return {...videoColorState(this)}; }
    }
    const videoColorState = value => {
        const state = videoColorStates.get(value);
        if (!state) throw new TypeError('Illegal VideoColorSpace receiver');
        return state;
    };
    const frameColor = (color, rgb) => new VideoColorSpace(color ?? (rgb ?
        {primaries: 'bt709', transfer: 'iec61966-2-1', matrix: 'rgb', fullRange: true} :
        {primaries: 'bt709', transfer: 'bt709', matrix: 'bt709', fullRange: false}));
    const frameInit = (value, buffer) => {
        const options = frameDictionary(value);
        const alpha = buffer ? 'keep' : frameEnum(options.alpha, ['keep', 'discard'], 'keep');
        const codedHeight = buffer ? frameUint(frameRequired(options.codedHeight, 'codedHeight')) : undefined;
        const codedWidth = buffer ? frameUint(frameRequired(options.codedWidth, 'codedWidth')) : undefined;
        const rawColorSpace = buffer ? options.colorSpace : undefined;
        const colorSpace = rawColorSpace === undefined ? undefined : frameColorOptions(rawColorSpace);
        const rawDisplayHeight = options.displayHeight;
        const displayHeight = rawDisplayHeight === undefined ? undefined : frameUint(rawDisplayHeight);
        const rawDisplayWidth = options.displayWidth;
        const displayWidth = rawDisplayWidth === undefined ? undefined : frameUint(rawDisplayWidth);
        const rawDuration = options.duration;
        const duration = rawDuration === undefined ? undefined :
            (buffer ? frameTime(rawDuration, true) : frameLongTime(rawDuration, true));
        const rawFlip = options.flip, flip = rawFlip === undefined ? false : Boolean(rawFlip);
        const format = buffer ? frameEnum(frameRequired(options.format, 'format'), frameFormats) : undefined;
        const layout = buffer ? frameLayoutOptions(options.layout) : undefined;
        const metadata = options.metadata;
        if (metadata !== undefined) frameDictionary(metadata);
        const rotation = frameRotation(options.rotation);
        const rawTimestamp = options.timestamp;
        const timestamp = rawTimestamp === undefined ? undefined :
            (buffer ? frameTime(rawTimestamp) : frameLongTime(rawTimestamp));
        if (buffer && timestamp === undefined) throw new TypeError('Frame timestamp is required');
        const transfer = buffer ? frameTransfers(options.transfer) : [];
        const visibleRect = frameRectOptions(options.visibleRect);
        if ((displayHeight === undefined) !== (displayWidth === undefined) ||
            displayHeight === 0 || displayWidth === 0)
            throw new TypeError('Frame display dimensions must be supplied together and positive');
        return {alpha, codedHeight, codedWidth, colorSpace, displayHeight, displayWidth,
            duration, flip, format, layout, rotation, timestamp, transfer, visibleRect};
    };
