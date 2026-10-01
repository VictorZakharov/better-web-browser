    // One-shot capability handoff to the structured clone implementation. It is
    // removed from globalThis after bootstrap, like the private Blob reader.
    globalThis.__videoFrameCloneBindings = {
        has: value => videoFrameStates.has(value),
        closed: value => !videoFrameStates.get(value)?.planes,
        detach: closeVideoFrame,
        snapshot(value) {
            const state = activeVideoFrame(value);
            const planes = frameFormatInfo(state.format);
            const size = state.planes.reduce((sum, plane) => sum + plane.length, 0);
            const pixels = new Uint8Array(size);
            let offset = 0;
            for (const plane of state.planes) { pixels.set(plane, offset); offset += plane.length; }
            return {format: state.format, width: state.width, height: state.height,
                visible: {...state.visible}, displayWidth: state.displayWidth, displayHeight: state.displayHeight,
                timestamp: state.timestamp, duration: state.duration,
                rotation: state.rotation, flip: state.flip, color: {...videoColorState(state.color)},
                planeCount: planes.length, pixels};
        },
        receive(record, pixels) {
            try {
                const options = frameInit({format: record.format, codedWidth: record.width,
                    codedHeight: record.height, visibleRect: record.visible,
                    displayWidth: record.displayWidth, displayHeight: record.displayHeight,
                    timestamp: record.timestamp, duration: record.duration ?? undefined,
                    rotation: record.rotation, flip: record.flip, colorSpace: record.color}, true);
                const state = frameStateOptions(options.codedWidth, options.codedHeight, options.format, options);
                const size = frameLayout(state.format, state.width, state.height).size;
                if (pixels.length !== size) throw new TypeError('Invalid frame transfer size');
                state.planes = frameReadPlanes(pixels, state.format, state.width, state.height);
                return makeVideoFrame(state);
            } catch (_) { throw frameError('Invalid VideoFrame clone record', 'DataCloneError'); }
        }
    };
