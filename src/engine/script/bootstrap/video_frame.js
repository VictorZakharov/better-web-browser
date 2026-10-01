    const videoFrameToken = Symbol('VideoFrame resource');
    const videoFrameState = frame => {
        const state = videoFrameStates.get(frame);
        if (!state) throw new TypeError('Illegal VideoFrame receiver');
        return state;
    };
    const activeVideoFrame = frame => {
        const state = videoFrameState(frame);
        if (!state.planes) throw frameError('VideoFrame is closed');
        return state;
    };
    const closeVideoFrame = frame => {
        const state = videoFrameState(frame);
        state.planes = null;
        state.width = 0; state.height = 0;
        state.visible = {x:0,y:0,width:0,height:0};
        state.displayWidth = 0; state.displayHeight = 0;
        state.rotation = 0; state.flip = false;
    };
    const makeVideoFrame = state => new VideoFrame(videoFrameToken, state);
    const frameStateOptions = (width, height, format, options) => {
        bitmapPixelBudget(width, height);
        const visible = frameRect(options.visibleRect, width, height);
        frameAligned(format, visible);
        const swapped = options.rotation === 90 || options.rotation === 270;
        return {width, height, format, visible,
            displayWidth: options.displayWidth ?? (swapped ? visible.height : visible.width),
            displayHeight: options.displayHeight ?? (swapped ? visible.width : visible.height),
            timestamp: options.timestamp ?? 0, duration: options.duration ?? null,
            rotation: options.rotation, flip: options.flip,
            color: frameColor(options.colorSpace, frameIsRGB(format))};
    };
    class VideoFrame {
        constructor(source, init = {}) {
            if (source === videoFrameToken) { videoFrameStates.set(this, init); return; }
            if (arguments.length === 0) throw new TypeError('VideoFrame requires a source');
            const buffer = source instanceof ArrayBuffer || ArrayBuffer.isView(source) ||
                (typeof SharedArrayBuffer === 'function' && source instanceof SharedArrayBuffer);
            const options = frameInit(init, buffer);
            if (buffer) {
                const bytes = frameBuffer(source);
                const state = frameStateOptions(options.codedWidth, options.codedHeight, options.format, options);
                state.planes = frameReadPlanes(bytes, state.format, state.width, state.height, options.layout);
                frameValidateTransfers(options.transfer);
                videoFrameStates.set(this, state);
                for (const transfer of options.transfer) host('arrayBufferDetach', transfer);
                return;
            }
            if (videoFrameStates.has(source)) {
                const previous = activeVideoFrame(source);
                const format = options.alpha === 'discard' ?
                    ({RGBA:'RGBX', BGRA:'BGRX', I420A:'I420', I422A:'I422', I444A:'I444'}[previous.format] ?? previous.format) : previous.format;
                const visible = frameRect(options.visibleRect ?? previous.visible, previous.width, previous.height);
                // Preserve the source's pixel aspect ratio when cropping or adding
                // orientation. Display dimensions describe the oriented rectangle.
                const baseSwapped = previous.rotation === 90 || previous.rotation === 270;
                const widthScale = (baseSwapped ? previous.displayHeight : previous.displayWidth) / previous.visible.width;
                const heightScale = (baseSwapped ? previous.displayWidth : previous.displayHeight) / previous.visible.height;
                const rotation = ((previous.rotation + (previous.flip ? -options.rotation : options.rotation)) % 360 + 360) % 360;
                const swapped = rotation === 90 || rotation === 270;
                const scaledWidth = Math.round(visible.width * widthScale);
                const scaledHeight = Math.round(visible.height * heightScale);
                const state = frameStateOptions(previous.width, previous.height, format,
                    {...options, visibleRect: options.visibleRect ?? previous.visible,
                        displayWidth: options.displayWidth ?? (swapped ? scaledHeight : scaledWidth),
                        displayHeight: options.displayHeight ?? (swapped ? scaledWidth : scaledHeight),
                        rotation,
                        flip: previous.flip !== options.flip,
                        timestamp: options.timestamp ?? previous.timestamp,
                        duration: options.duration ?? previous.duration,
                        colorSpace: {...videoColorState(previous.color)}});
                state.planes = format !== previous.format && previous.planes.length === 4 ? previous.planes.slice(0, 3) : previous.planes;
                videoFrameStates.set(this, state);
                return;
            }
            if (options.timestamp === undefined) throw new TypeError('Canvas source requires a timestamp');
            const image = imageSourceSnapshot(source);
            const format = options.alpha === 'discard' ? 'RGBX' : 'RGBA';
            const state = frameStateOptions(image.width, image.height, format, options);
            state.planes = [new Uint8Array(image.pixels)];
            videoFrameStates.set(this, state);
        }
        get format() { const state = videoFrameState(this); return state.planes ? state.format : null; }
        get codedWidth() { const state = videoFrameState(this); return state.planes ? state.width : 0; }
        get codedHeight() { const state = videoFrameState(this); return state.planes ? state.height : 0; }
        get codedRect() {
            const state = videoFrameState(this);
            return state.planes ? new DOMRectReadOnly(0, 0, state.width, state.height) : null;
        }
        get visibleRect() {
            const state = videoFrameState(this), rect = state.visible;
            return state.planes ? new DOMRectReadOnly(rect.x, rect.y, rect.width, rect.height) : null;
        }
        get displayWidth() { const state = videoFrameState(this); return state.planes ? state.displayWidth : 0; }
        get displayHeight() { const state = videoFrameState(this); return state.planes ? state.displayHeight : 0; }
        get timestamp() { return videoFrameState(this).timestamp; }
        get duration() { return videoFrameState(this).duration; }
        get rotation() { return videoFrameState(this).rotation; }
        get flip() { return videoFrameState(this).flip; }
        get colorSpace() { return videoFrameState(this).color; }
        allocationSize(options = {}) {
            videoFrameState(this);
            const converted = frameCopyDictionary(options);
            const state = activeVideoFrame(this);
            return frameCopyOptions(converted, state).size;
        }
        copyTo(destination, options = {}) {
            // Promise-returning Web IDL operations reject conversion failures.
            try {
                videoFrameState(this);
                if (arguments.length === 0) throw new TypeError('Frame destination is required');
                const bytes = frameBuffer(destination);
                const converted = frameCopyDictionary(options);
                const state = activeVideoFrame(this);
                const computed = frameCopyOptions(converted, state);
                // Snapshot before settlement. close() does not cancel an accepted copy.
                const result = frameCopyPlanes(state, bytes, computed);
                return Promise.resolve(result);
            } catch (error) { return Promise.reject(error); }
        }
        clone() { return makeVideoFrame({...activeVideoFrame(this)}); }
        close() {
            closeVideoFrame(this);
        }
        metadata() { activeVideoFrame(this); return {}; }
    }
    Object.defineProperty(VideoFrame.prototype, Symbol.toStringTag, {value: 'VideoFrame', configurable: true});
    Object.defineProperty(VideoColorSpace.prototype, Symbol.toStringTag, {value: 'VideoColorSpace', configurable: true});
    const frameIDL = constructors => {
        for (const constructor of constructors) for (const key of Object.getOwnPropertyNames(constructor.prototype)) {
            if (key === 'constructor') continue;
            const descriptor = Object.getOwnPropertyDescriptor(constructor.prototype, key);
            Object.defineProperty(constructor.prototype, key, {...descriptor, enumerable:true});
        }
    };
    frameIDL([VideoFrame, VideoColorSpace]);
    Object.assign(globalThis, {VideoFrame, VideoColorSpace});
