    // WebVTT file parser: header, cue timings/settings, comments, and safe cue-text DOM.
    // https://www.w3.org/TR/webvtt1/#webvtt-parser-algorithm
    const MAX_VTT_CHARS = 2 * 1024 * 1024, MAX_VTT_CUES = 10000;
    const vttTimestamp = text => {
        const parts = text.split(':');
        if (parts.length !== 2 && parts.length !== 3) return null;
        const seconds = parts.pop();
        if (!/^\d{2}\.\d{3}$/.test(seconds) || !/^\d{2}$/.test(parts[parts.length - 1])) return null;
        const minute = Number(parts.pop()), hour = parts.length ? Number(parts.pop()) : 0;
        const second = Number(seconds);
        if (minute > 59 || second >= 60 || !Number.isSafeInteger(hour)) return null;
        return hour * 3600 + minute * 60 + second;
    };
    const vttPercentage = text => {
        if (!/^(?:\d{1,2}(?:\.\d+)?|100(?:\.0+)?)%$/.test(text)) return null;
        const result = Number(text.slice(0, -1));
        return result <= 100 ? result : null;
    };
    const applyVttSetting = (cue, token) => {
        const colon = token.indexOf(':');
        if (colon <= 0) return;
        const key = token.slice(0, colon), raw = token.slice(colon + 1);
        const [value, align] = raw.split(',');
        switch (key) {
            case 'vertical':
                if (value === 'rl' || value === 'lr') cue.vertical = value;
                break;
            case 'line': {
                const percent = vttPercentage(value);
                if (percent !== null) { cue.snapToLines = false; cue.line = percent; }
                else if (/^-?\d+$/.test(value)) { cue.snapToLines = true; cue.line = Number(value); }
                if (['start', 'center', 'end'].includes(align)) cue.lineAlign = align;
                break;
            }
            case 'position': {
                const percent = vttPercentage(value);
                if (percent !== null) cue.position = percent;
                if (['line-left', 'center', 'line-right', 'auto'].includes(align)) cue.positionAlign = align;
                break;
            }
            case 'size': {
                const percent = vttPercentage(value);
                if (percent !== null) cue.size = percent;
                break;
            }
            case 'align':
                if (['start', 'center', 'end', 'left', 'right'].includes(value)) cue.align = value;
                break;
        }
    };
    const parseVttTiming = line => {
        const marker = line.indexOf('-->');
        if (marker < 0) return null;
        const start = vttTimestamp(line.slice(0, marker).trim());
        const tail = line.slice(marker + 3).trim().split(/[ \t]+/);
        const end = vttTimestamp(tail.shift());
        return start === null || end === null || end < start ? null : { start, end, settings: tail };
    };
    const parseWebVtt = input => {
        input = String(input);
        if (input.length > MAX_VTT_CHARS) throw new Error('WebVTT file exceeds resource budget');
        const lines = input.replace(/^\ufeff/, '').replace(/\r\n?/g, '\n').split('\n');
        if (!/^WEBVTT(?:[ \t].*)?$/.test(lines[0])) throw new Error('Invalid WebVTT header');
        let line = 1;
        while (line < lines.length && lines[line].trim() !== '') line++;
        const cues = [];
        while (line < lines.length) {
            while (line < lines.length && lines[line].trim() === '') line++;
            if (line >= lines.length) break;
            const block = [];
            while (line < lines.length && lines[line].trim() !== '') block.push(lines[line++]);
            if (!block.length || /^(NOTE|STYLE|REGION)(?:[ \t]|$)/.test(block[0])) continue;
            let id = '', timing = parseVttTiming(block[0]);
            if (!timing && block.length > 1) { id = block.shift(); timing = parseVttTiming(block[0]); }
            if (!timing) continue;
            const cue = new VTTCue(timing.start, timing.end, block.slice(1).join('\n'));
            cue.id = id;
            for (const setting of timing.settings) applyVttSetting(cue, setting);
            cues.push(cue);
            if (cues.length >= MAX_VTT_CUES) throw new Error('WebVTT file exceeds cue budget');
        }
        cues.sort((a, b) => a.startTime - b.startTime || b.endTime - a.endTime);
        return cues;
    };
    const vttEntities = { amp: '&', lt: '<', gt: '>', lrm: '\u200e', rlm: '\u200f', nbsp: '\u00a0' };
    const decodeVttText = value => value.replace(/&([a-z]+);/gi,
        (source, name) => vttEntities[name.toLowerCase()] ?? source);
    const parseVttCueText = text => {
        const fragment = document.createDocumentFragment(), stack = [fragment];
        const appendText = value => {
            if (value) stack[stack.length - 1].appendChild(document.createTextNode(decodeVttText(value)));
        };
        const tokens = String(text).split(/(<[^>]*>)/g);
        for (const token of tokens) {
            if (!token.startsWith('<') || !token.endsWith('>')) { appendText(token); continue; }
            const close = /^<\/([biur]|rt|ruby|c|v|lang)>$/i.exec(token);
            if (close) {
                const name = close[1].toLowerCase();
                for (let at = stack.length - 1; at > 0; at--)
                    if (stack[at].__vttName === name) { stack.length = at; break; }
                continue;
            }
            const open = /^<(b|i|u|ruby|rt|c(?:\.[\w-]+)*|v(?:\s+[^>]*)?|lang(?:\s+[^>]*)?)>$/i.exec(token);
            if (!open) continue;
            const raw = open[1], name = raw.split(/[ .]/)[0].toLowerCase();
            const element = document.createElement(['c', 'v', 'lang'].includes(name) ? 'span' : name);
            if (name === 'c') element.className = raw.slice(1).replaceAll('.', ' ').trim();
            if (name === 'v') element.setAttribute('title', raw.slice(1).trim());
            if (name === 'lang') element.setAttribute('lang', raw.slice(4).trim());
            // Parser-owned annotation, not author-visible markup or a script sink.
            Object.defineProperty(element, '__vttName', { value: name });
            stack[stack.length - 1].appendChild(element);
            stack.push(element);
        }
        return fragment;
    };
