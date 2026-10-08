    // Reuse the existing resvg/usvg provider's pinned SVG 2 grammar. The owned
    // command stream retains original coordinates for our Canvas flattening.
    const svgPathArc = (path, x0, y0, rx, ry, degrees, large, sweep, x1, y1) => {
        rx = canvasPrivateMath.abs(rx); ry = canvasPrivateMath.abs(ry);
        if (x0 === x1 && y0 === y1) return;
        if (rx === 0 || ry === 0) { lineCanvasPath(path, x1, y1); return; }
        const rotation = degrees * canvasPrivateMath.PI / 180;
        const cosine = canvasPrivateMath.cos(rotation), sine = canvasPrivateMath.sin(rotation);
        const dx = (x0 - x1) / 2, dy = (y0 - y1) / 2;
        const middleX = cosine * dx + sine * dy;
        const middleY = -sine * dx + cosine * dy;
        let radiiScale = middleX * middleX / (rx * rx) + middleY * middleY / (ry * ry);
        if (radiiScale > 1) {
            radiiScale = canvasPrivateMath.sqrt(radiiScale);
            rx *= radiiScale; ry *= radiiScale;
        }
        const numerator = canvasPrivateMath.max(0, rx * rx * ry * ry - rx * rx * middleY * middleY -
            ry * ry * middleX * middleX);
        const denominator = rx * rx * middleY * middleY + ry * ry * middleX * middleX;
        const factor = (large === sweep ? -1 : 1) * canvasPrivateMath.sqrt(numerator / denominator);
        const centerXPrime = factor * rx * middleY / ry;
        const centerYPrime = -factor * ry * middleX / rx;
        const centerX = cosine * centerXPrime - sine * centerYPrime + (x0 + x1) / 2;
        const centerY = sine * centerXPrime + cosine * centerYPrime + (y0 + y1) / 2;
        const startX = (middleX - centerXPrime) / rx;
        const startY = (middleY - centerYPrime) / ry;
        const endX = (-middleX - centerXPrime) / rx;
        const endY = (-middleY - centerYPrime) / ry;
        const start = canvasPrivateMath.atan2(startY, startX);
        let delta = canvasPrivateMath.atan2(startX * endY - startY * endX,
            startX * endX + startY * endY);
        if (sweep && delta < 0) delta += 2 * canvasPrivateMath.PI;
        if (!sweep && delta > 0) delta -= 2 * canvasPrivateMath.PI;
        ellipseCanvasPath(path, centerX, centerY, rx, ry, rotation, start, start + delta,
            !sweep);
        // Avoid accumulated trigonometric error at the endpoint of connected segments.
        const points = path.subpaths[path.current].points;
        points[points.length - 1] = [x1, y1];
    };
    const parseCanvasSvgPath = source => {
        if (source.length > 131072)
            throw new DOMException('SVG path data exceeds the geometry budget', 'NotSupportedError');
        const segments = canvasCurveHost('canvasSvgPathSegments', source);
        if (!segments) throw new DOMException('SVG path data exceeds the parsing budget', 'NotSupportedError');
        const path = newCanvasPath();
        let x = 0, y = 0, startX = 0, startY = 0;
        let lastCurve = '', cubicControl = null, quadraticControl = null;
        let values = [], index = 0;
        const readNumber = () => values[index++];
        const endpoint = relative => {
            const px = readNumber(), py = readNumber();
            return relative ? [x + px, y + py] : [px, py];
        };
        for (const segment of segments) {
            const command = segment[0];values = segment;index = 1;
            const type = command.toUpperCase();
            if (type === 'Z') {
                closeCanvasPath(path); x = startX; y = startY;
                lastCurve = ''; cubicControl = quadraticControl = null;
                continue;
            }
            const relative = command === command.toLowerCase();
            if (type === 'M') {
                [x, y] = endpoint(relative);
                moveCanvasPath(path, x, y); startX = x; startY = y;
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
                const large = readNumber(), sweep = readNumber(), end = endpoint(relative);
                svgPathArc(path, x, y, rx, ry, rotation, large, sweep, ...end);
                [x, y] = end;
            }
            lastCurve = type;
        }
        return path;
    };
