    // A MIME claim is meaningful only when the contained media worker can demux and decode it.
    // A codec-less container remains "maybe" when it can carry formats we cannot decode.
    // https://html.spec.whatwg.org/multipage/media.html#dom-navigator-canplaytype
    const supportedMediaType = type => {
        const source = String(type).trim().toLowerCase();
        if (!source) return '';
        const [essence, ...parameters] = source.split(';').map(part => part.trim());
        // Both M4A aliases identify the same AAC-bearing ISO-BMFF files decoded by the
        // media worker. Keep container and codec checks identical to audio/mp4.
        const mp4 = essence === 'video/mp4' || essence === 'audio/mp4'
            || essence === 'audio/m4a' || essence === 'audio/x-m4a'
            || essence === 'application/mp4';
        const wave = essence === 'audio/wav' || essence === 'audio/wave'
            || essence === 'audio/x-wav' || essence === 'audio/vnd.wave';
        const mpeg = essence === 'audio/mpeg';
        const aac = essence === 'audio/aac';
        // RFC 5334 names Ogg's Vorbis codec identifier. Codec-less Ogg can also
        // contain Opus, FLAC or video, so it cannot be reported as "probably".
        const ogg = essence === 'audio/ogg';
        // RFC 9639 registers native FLAC as audio/flac; audio/x-flac is its deprecated
        // alias. This decoder does not imply support for FLAC in Ogg or another container.
        const flac = essence === 'audio/flac' || essence === 'audio/x-flac';
        if (flac) return parameters.length ? '' : 'maybe';
        if (!mp4 && !wave && !mpeg && !aac && !ogg) return '';

        const codecParameters = parameters.filter(parameter => /^codecs(?:\s|=|$)/.test(parameter));
        if (codecParameters.length > 1) return '';
        if (!codecParameters.length) return 'maybe';
        if (!/^codecs\s*=/.test(codecParameters[0])) return '';
        const codecValue = codecParameters[0].slice(codecParameters[0].indexOf('=') + 1)
            .trim().replace(/^(?:"([^"]*)"|'([^']*)')$/, (_, double, single) => double ?? single);
        const codecs = codecValue.split(',').map(codec => codec.trim());
        if (codecs.some(codec => !codec)) return '';

        if (wave) return codecs.length === 1 && (codecs[0] === '1' || codecs[0] === 'pcm')
            ? 'probably' : '';
        if (mpeg) return codecs.length === 1 && codecs[0] === 'mp3' ? 'probably' : '';
        if (aac) return codecs.length === 1 && codecs[0] === 'mp4a.40.2' ? 'probably' : '';
        if (ogg) return codecs.length === 1 && codecs[0] === 'vorbis' ? 'probably' : '';
        const hasAudio = codecs.includes('mp4a.40.2');
        const hasVideo = codecs.some(codec => /^avc1\.[0-9a-f]{6}$/.test(codec));
        if (codecs.length !== Number(hasAudio) + Number(hasVideo)) return '';
        // The contained worker owns a monotonic playback clock for complete video-only MP4.
        // A video-only stream must not be advertised under the audio/mp4 essence.
        return hasAudio || (hasVideo && !essence.startsWith('audio/')) ? 'probably' : '';
    };
