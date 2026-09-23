    // SVG 2 path-data grammar. Parse commands before flattening so shorthand control points,
    // compact arc flags, and relative coordinates all use the original geometric points.
    const svgPathNumber = /[+-]?(?:(?:\d+\.?\d*)|(?:\.\d+))(?:[eE][+-]?\d+)?/y;
    const svgPathError = () => new DOMException('Invalid SVG path data', 'SyntaxError');
    const svgPathArc = (path, x0, y0, rx, ry, degrees, large, sweep, x1, y1) => {
        rx = Math.abs(rx); ry = Math.abs(ry);
        if (x0 === x1 && y0 === y1) return;
        if (rx === 0 || ry === 0) { lineCanvasPath(path, x1, y1); return; }
        const rotation = degrees * Math.PI / 180;
        const cosine = Math.cos(rotation), sine = Math.sin(rotation);
        const dx = (x0 - x1) / 2, dy = (y0 - y1) / 2;
        const middleX = cosine * dx + sine * dy;
        const middleY = -sine * dx + cosine * dy;
        let radiiScale = middleX * middleX / (rx * rx) + middleY * middleY / (ry * ry);
        if (radiiScale > 1) {
            radiiScale = Math.sqrt(radiiScale);
            rx *= radiiScale; ry *= radiiScale;
        }
        const numerator = Math.max(0, rx * rx * ry * ry - rx * rx * middleY * middleY -
            ry * ry * middleX * middleX);
        const denominator = rx * rx * middleY * middleY + ry * ry * middleX * middleX;
        const factor = (large === sweep ? -1 : 1) * Math.sqrt(numerator / denominator);
        const centerXPrime = factor * rx * middleY / ry;
        const centerYPrime = -factor * ry * middleX / rx;
        const centerX = cosine * centerXPrime - sine * centerYPrime + (x0 + x1) / 2;
        const centerY = sine * centerXPrime + cosine * centerYPrime + (y0 + y1) / 2;
        const startX = (middleX - centerXPrime) / rx;
        const startY = (middleY - centerYPrime) / ry;
        const endX = (-middleX - centerXPrime) / rx;
        const endY = (-middleY - centerYPrime) / ry;
        const start = Math.atan2(startY, startX);
        let delta = Math.atan2(startX * endY - startY * endX,
            startX * endX + startY * endY);
        if (sweep && delta < 0) delta += 2 * Math.PI;
        if (!sweep && delta > 0) delta -= 2 * Math.PI;
        ellipseCanvasPath(path, centerX, centerY, rx, ry, rotation, start, start + delta,
            !sweep);
        // Avoid accumulated trigonometric error at the endpoint of connected segments.
        const points = path.subpaths[path.current].points;
        points[points.length - 1] = [x1, y1];
    };
    const parseCanvasSvgPath = source => {
        if (source.length > 131072)
            throw new DOMException('SVG path data exceeds the geometry budget', 'NotSupportedError');
        const path = newCanvasPath();
        let offset = 0, command = '', x = 0, y = 0, startX = 0, startY = 0;
        let lastCurve = '', cubicControl = null, quadraticControl = null;
        const skipSeparators = () => {
            while (offset < source.length && /[\s,]/.test(source[offset])) offset++;
        };
        const readNumber = () => {
            skipSeparators();
            svgPathNumber.lastIndex = offset;
            const match = svgPathNumber.exec(source);
            if (!match) throw svgPathError();
            offset += match[0].length;
            const value = Number(match[0]);
            if (!Number.isFinite(value)) throw svgPathError();
            return value;
        };
        const readFlag = () => {
            skipSeparators();
            if (source[offset] !== '0' && source[offset] !== '1') throw svgPathError();
            return Number(source[offset++]);
        };
        const endpoint = relative => {
            const px = readNumber(), py = readNumber();
            return relative ? [x + px, y + py] : [px, py];
        };
        skipSeparators();
        while (offset < source.length) {
            if (/[A-Za-z]/.test(source[offset])) command = source[offset++];
            else if (!command) throw svgPathError();
            const type = command.toUpperCase();
            if (!'MLHVCSQTAZ'.includes(type)) throw svgPathError();
            if (path.current === null && type !== 'M') throw svgPathError();
            if (type === 'Z') {
                closeCanvasPath(path); x = startX; y = startY;
                lastCurve = ''; cubicControl = quadraticControl = null;
                command = '';
                skipSeparators();
                continue;
            }
            const relative = command === command.toLowerCase();
            if (type === 'M') {
                [x, y] = endpoint(relative);
                moveCanvasPath(path, x, y); startX = x; startY = y;
                command = relative ? 'l' : 'L';
            } else if (type === 'L') {
                [x, y] = endpoint(relative);
                lineCanvasPath(path, x, y);
            } else if (type === 'H') {
                const value = readNumber(); x = relative ? x + value : value;
                lineCanvasPath(path, x, y);
            } else if (type === 'V') {
                const value = readNumber(); y = relative ? y + value : value;
                lineCanvasPath(path, x, y);
            } else if (type === 'C') {
                const first = endpoint(relative), second = endpoint(relative), end = endpoint(relative);
                curveCanvasPath(path, [...first, ...second, ...end], true);
                cubicControl = second; [x, y] = end;
            } else if (type === 'S') {
                const first = lastCurve === 'C' || lastCurve === 'S' ?
                    [2 * x - cubicControl[0], 2 * y - cubicControl[1]] : [x, y];
                const second = endpoint(relative), end = endpoint(relative);
                curveCanvasPath(path, [...first, ...second, ...end], true);
                cubicControl = second; [x, y] = end;
            } else if (type === 'Q') {
                const control = endpoint(relative), end = endpoint(relative);
                curveCanvasPath(path, [...control, ...end], false);
                quadraticControl = control; [x, y] = end;
            } else if (type === 'T') {
                const control = lastCurve === 'Q' || lastCurve === 'T' ?
                    [2 * x - quadraticControl[0], 2 * y - quadraticControl[1]] : [x, y];
                const end = endpoint(relative);
                curveCanvasPath(path, [...control, ...end], false);
                quadraticControl = control; [x, y] = end;
            } else {
                const rx = readNumber(), ry = readNumber(), rotation = readNumber();
                const large = readFlag(), sweep = readFlag(), end = endpoint(relative);
                svgPathArc(path, x, y, rx, ry, rotation, large, sweep, ...end);
                [x, y] = end;
            }
            lastCurve = type;
            skipSeparators();
            if (offset < source.length && !/[A-Za-z+\-.\d]/.test(source[offset]))
                throw svgPathError();
        }
        return path;
    };
