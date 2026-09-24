    // CSS Transforms §11: interpolate compatible transform functions, padding
    // `none` with identity operations. The current painter implements only 2D
    // translation, so unsupported rotation/scale lists keep discrete behavior.
    // https://www.w3.org/TR/css-transforms-1/#interpolation-of-transforms
    const parseAnimationTransform = source => {
        const text = String(source).trim();
        if (text.toLowerCase() === 'none') return [];
        const functions = [];
        const expression = /(translatex|translatey|translate|matrix)\(\s*([^()]*)\s*\)/ig;
        let cursor = 0, match;
        while ((match = expression.exec(text))) {
            if (text.slice(cursor, match.index).trim()) return null;
            cursor = expression.lastIndex;
            const name = match[1].toLowerCase();
            const arguments_ = match[2].includes(',') ? match[2].split(',') : match[2].trim().split(/\s+/);
            if (name === 'matrix') {
                const numbers = arguments_.map(Number);
                if (numbers.length !== 6 || !numbers.every(Number.isFinite) ||
                    numbers[0] !== 1 || numbers[1] !== 0 || numbers[2] !== 0 || numbers[3] !== 1)
                    return null;
                functions.push({name, x: {value: numbers[4], unit: ''},
                    y: {value: numbers[5], unit: ''}});
                continue;
            }
            if (arguments_.length < 1 || arguments_.length > (name === 'translate' ? 2 : 1)) return null;
            const values = arguments_.map(argument => {
                const number = animationNumber.exec(argument.trim());
                if (!number || !['', 'px', '%', 'em', 'rem', 'vw', 'vh'].includes(number[2])) return null;
                return {value: Number(number[1]), unit: number[2]};
            });
            if (values.some(value => value === null)) return null;
            const zero = {value: 0, unit: values[0].unit};
            functions.push({name, x: name === 'translatey' ? zero : values[0],
                y: name === 'translatex' ? zero : values[1] ?? zero});
        }
        return cursor === text.length && functions.length ? functions : null;
    };
    const interpolateAnimationTransform = (from, to, progress) => {
        let first = parseAnimationTransform(from), last = parseAnimationTransform(to);
        if (first === null || last === null) return null;
        if (!first.length && last.length) first = last.map(function_ => ({
            name: function_.name, x: {...function_.x, value: 0}, y: {...function_.y, value: 0}
        }));
        if (!last.length && first.length) last = first.map(function_ => ({
            name: function_.name, x: {...function_.x, value: 0}, y: {...function_.y, value: 0}
        }));
        if (first.length !== last.length) return null;
        const result = [];
        for (let index = 0; index < first.length; index++) {
            const a = first[index], b = last[index];
            if (a.name !== b.name || a.x.unit !== b.x.unit || a.y.unit !== b.y.unit) return null;
            const x = a.x.value + (b.x.value - a.x.value) * progress;
            const y = a.y.value + (b.y.value - a.y.value) * progress;
            const value = number => String(Math.round(number * 10000) / 10000);
            result.push(`translate(${value(x)}${a.x.unit || 'px'}, ${value(y)}${a.y.unit || 'px'})`);
        }
        return result.length ? result.join(' ') : 'none';
    };
