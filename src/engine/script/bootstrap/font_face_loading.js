    // All observable completion/status updates use the font loading task source,
    // not the promise microtask which delivered fetched bytes.
    // https://drafts.csswg.org/css-font-loading-3/#font-face-load
    const finishFontFaceLoad = (face, data, error, tryNext = null) => {
        const state = fontState(face);
        if (state.status !== 'loading') return;
        try {
            if (error) throw error;
            const weight = fontWeight(state.descriptors.weight);
            const italic = fontStyle(state.descriptors.style);
            if (!fontHost('fontFaceValidate', state.id, state.descriptors.family,
                weight, italic, data, state.descriptors.unicodeRange, state.descriptors.featureSettings))
                throw new DOMException('FontFace source could not be decoded', 'SyntaxError');
            state.bytes = data;
            state.status = 'loaded';
            fontInstall(face);
            state.resolveLoaded(face);
            if (state.owner) markFontSettled(state.owner, face, true);
        } catch (failure) {
            if (tryNext) {
                state.bytes = null;
                state.status = 'loading';
                tryNext();
                return;
            }
            state.status = 'error';
            // Exhausting a URL source list is a load failure, even if the last
            // response contained invalid font bytes. Buffer parsing is syntax.
            // https://drafts.csswg.org/css-font-loading/#font-face-load
            state.rejectLoaded(state.urls === null ? failure :
                new DOMException('No usable FontFace source', 'NetworkError'));
            if (state.owner) markFontSettled(state.owner, face, false);
        }
    };
    const beginFontFaceLoad = face => {
        const state = fontState(face);
        if (state.status !== 'unloaded') return;
        state.status = 'loading';
        if (state.owner) markFontLoading(state.owner, face);
        if (state.bytes) {
            queueFontTask(() => finishFontFaceLoad(face, state.bytes, null));
            return;
        }
        // One source at a time, advancing after fetch OR decode failure. Do not
        // expose an intermediate error event or resolve ready between candidates.
        // local() aliases are parsed, but currently treated as unavailable.
        const attempt = index => {
            if (index >= state.urls.length) {
                queueFontTask(() => finishFontFaceLoad(face, null,
                    new DOMException('No usable FontFace source', 'NetworkError')));
                return;
            }
            const next = index + 1 < state.urls.length ? () => attempt(index + 1) : null;
            fontFetch(state.urls[index], {mode:'cors'}).then(response => {
                if (!response.ok) throw new DOMException('FontFace download failed', 'NetworkError');
                return response.arrayBuffer();
            }).then(buffer => {
                const data = new fontBytes(buffer);
                queueFontTask(() => finishFontFaceLoad(face, data, null, next));
            }, failure => {
                const error = failure instanceof DOMException ? failure :
                    new DOMException('FontFace download failed', 'NetworkError');
                queueFontTask(() => finishFontFaceLoad(face, null, error, next));
            });
        };
        attempt(0);
    };
