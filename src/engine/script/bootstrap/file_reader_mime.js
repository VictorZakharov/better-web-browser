// The File API obtains the charset from a parsed MIME type, not a substring
// search: quoted parameter values may themselves contain `;charset=`.
(() => {
    'use strict';
    const token = /^[!#$%&'*+.^_`|~0-9A-Za-z-]+$/;
    const whitespace = character => character === ' ' || character === '\t' ||
        character === '\n' || character === '\r';
    const lowercase = value => value.replace(/[A-Z]/g,
        character => character.toLowerCase());
    const validValue = value => [...value].every(character => {
        const code = character.charCodeAt(0);
        return code === 9 || (code >= 0x20 && code <= 0x7e) ||
            (code >= 0x80 && code <= 0xff);
    });

    const charset = input => {
        const text = String(input).trim();
        const firstParameter = text.indexOf(';');
        const essence = (firstParameter < 0 ? text : text.slice(0, firstParameter))
            .trimEnd();
        const slash = essence.indexOf('/');
        if (slash <= 0 || slash === essence.length - 1 ||
            !token.test(essence.slice(0, slash)) ||
            !token.test(essence.slice(slash + 1))) return '';
        let position = firstParameter < 0 ? text.length : firstParameter;
        while (position < text.length) {
            ++position; // parameter delimiter
            while (whitespace(text[position])) ++position;
            const nameStart = position;
            while (position < text.length && text[position] !== ';' &&
                text[position] !== '=') ++position;
            const name = lowercase(text.slice(nameStart, position));
            if (text[position] !== '=') continue;
            ++position;
            let value = '';
            if (text[position] === '"') {
                ++position;
                while (position < text.length) {
                    const character = text[position++];
                    if (character === '"') break;
                    if (character === '\\' && position < text.length)
                        value += text[position++];
                    else value += character;
                }
                while (position < text.length && text[position] !== ';') ++position;
            } else {
                const valueStart = position;
                while (position < text.length && text[position] !== ';') ++position;
                value = text.slice(valueStart, position).trimEnd();
                if (!value) continue;
            }
            // MIME parsing stores only the first syntactically valid value for
            // a parameter name. We need only `charset`, so return at that point.
            if (name === 'charset' && token.test(name) && validValue(value))
                return value;
        }
        return '';
    };
    Object.defineProperty(globalThis, '__fileReaderMimeCharset',
        {configurable: true, value: charset});
})();
