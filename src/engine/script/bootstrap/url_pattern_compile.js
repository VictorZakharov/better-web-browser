// A deliberately bounded subset of the URL Pattern Standard's part grammar.
// Kept private to the bootstrap closure; author code cannot replace the compiler.
// https://urlpattern.spec.whatwg.org/#pattern-strings
const compileURLPatternComponent = (name, input, ignoreCase) => {
    const maxLength = 4096, maxGroups = 16;
    if (input.length > maxLength) throw new TypeError('URLPattern component is too long');
    const escapeRegExp = value => value.replace(/[\\^$.*+?()[\]{}|]/g, '\\$&');
    const delimiter = name === 'pathname' ? '/' : name === 'hostname' ? '.' : null;
    let source = '', pattern = '^', unnamed = 0, wildcardCount = 0;
    let previousDelimiter = '', segmentHasCapture = false;
    const groups = [], used = new Set();
    const appendLiteral = (character, escaped = false) => {
        const value = name === 'protocol' || name === 'hostname' ? character.toLowerCase() : character;
        source += escaped ? '\\' + value : value;
        pattern += escapeRegExp(value);
        previousDelimiter = !escaped && value === delimiter ? value : '';
        if (previousDelimiter) segmentHasCapture = false;
    };
    for (let i = 0; i < input.length; i++) {
        const character = input[i];
        if (character === '\\') {
            if (++i >= input.length) throw new TypeError('Incomplete URLPattern escape');
            appendLiteral(input[i], true);
            continue;
        }
        if (character === '*') {
            if (++wildcardCount > 1 || name === 'pathname' && i !== input.length - 1 ||
                segmentHasCapture || name !== 'pathname' && groups.length)
                throw new TypeError('Ambiguous URLPattern wildcard');
            const key = String(unnamed++);
            groups.push(key);
            if (groups.length > maxGroups) throw new TypeError('Too many URLPattern groups');
            source += '*'; pattern += '(.*)'; previousDelimiter = '';
            segmentHasCapture = true;
            continue;
        }
        if (character === ':' && /[A-Za-z_]/.test(input[i + 1] || '')) {
            let end = i + 2;
            while (end < input.length && /[A-Za-z_0-9]/.test(input[end])) end++;
            const key = input.slice(i + 1, end);
            const modifier = '?+*'.includes(input[end]) ? input[end] : '';
            const after = end + (modifier ? 1 : 0);
            const startsSegment = i === 0 || input[i - 1] === delimiter;
            const endsSegment = after === input.length || input[after] === delimiter;
            const standalone = startsSegment && endsSegment;
            if (used.has(key)) throw new TypeError('Duplicate URLPattern group name');
            // One capture per segment avoids adjacent ambiguous groups.
            // A modified capture must be a whole segment so its automatic prefix has
            // the same meaning as the standard's part-list prefix code point.
            if (delimiter && segmentHasCapture || !delimiter && groups.length ||
                name === 'hostname' && wildcardCount || modifier && !standalone ||
                (modifier === '+' || modifier === '*') && !delimiter)
                throw new TypeError('Ambiguous URLPattern capture');
            used.add(key); groups.push(key);
            if (groups.length > maxGroups) throw new TypeError('Too many URLPattern groups');
            const segmentUnit = name === 'pathname' ? '[^/]' :
                name === 'hostname' ? '[^.]' : '.';
            const atom = segmentUnit + '+?';
            // The standard's automatic prefix is '/' only for pathnames.
            // Hostname '.' is a segment delimiter, not an optional/repeated prefix.
            const automaticPrefix = name === 'pathname' && previousDelimiter === '/';
            const repeated = modifier === '+' || modifier === '*' ?
                automaticPrefix ? atom + '(?:/' + atom + ')*' :
                    segmentUnit + modifier : atom;
            if ((modifier === '?' || modifier === '*') && automaticPrefix) {
                pattern = pattern.slice(0, -escapeRegExp(previousDelimiter).length);
                pattern += '(?:' + escapeRegExp(previousDelimiter) + '(' + repeated + '))?';
            } else pattern += '(' + repeated + ')' + (modifier === '?' ? '?' : '');
            source += ':' + key + modifier;
            i = after - 1; previousDelimiter = '';
            segmentHasCapture = true;
            continue;
        }
        if ('{}()+?'.includes(character))
            throw new TypeError('Unsupported URLPattern group or modifier syntax');
        appendLiteral(character);
    }
    pattern += '$';
    return { source, regexp: new RegExp(pattern, ignoreCase ? 'i' : ''), groups };
};
