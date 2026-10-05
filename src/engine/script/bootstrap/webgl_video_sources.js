    // Fetch policy and current-frame bytes are renderer-owned. Never accept a
    // forged prototype, writable videoWidth, src URL, or crossOrigin as authority.
    const webGlIsVideoSource = source => typeof videoElementBrands !== 'undefined' &&
        videoElementBrands.has(source);
    const webGlVideoSnapshot = source => {
        const snapshot=host('mediaVideoSnapshot',nodeId(source));
        if (snapshot==='tainted')
            throw new DOMException('Video pixels are not origin-clean','SecurityError');
        if (!snapshot) throw new DOMException('Video has no current frame','InvalidStateError');
        return {width:snapshot[0],height:snapshot[1],pixels:snapshot[2]};
    };
    const webGlSourceSnapshot = (context,source) => {
        try {
            return webGlIsVideoSource(source) ? webGlVideoSnapshot(source) : imageSourceSnapshot(source,true);
        } catch (error) {
            // WebGL's DOM upload contract uses GL errors for unavailable/detached
            // bitmaps, but keeps origin violations as synchronous SecurityError.
            if (error instanceof DOMException && error.name==='InvalidStateError') {
                webGlError(context,0x0501); return null;
            }
            throw error;
        }
    };
