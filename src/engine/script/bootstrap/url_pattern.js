(() => {
    'use strict';
    // URL Pattern Standard: component matching and URLPatternInit processing.
    // https://urlpattern.spec.whatwg.org/
    const { usv, parts } = globalThis.__urlInternals;
    const URLConstructor = globalThis.URL;
    const resolve = (value, base) => __hostCall('strictResolveUrl', value, base);
    const names = ['protocol', 'username', 'password', 'hostname', 'port',
        'pathname', 'search', 'hash'];
    const slots = new WeakMap();
    const MAX_PATTERN_LENGTH = 4096, MAX_COMPONENT_LENGTH = 8192, MAX_GROUPS = 16;
    const specialPorts = { http: '80', https: '443', ws: '80', wss: '443', ftp: '21' };
    const escapeRegExp = value => value.replace(/[\\^$.*+?()[\]{}|]/g, '\\$&');
    const escapePattern = value => value.replace(/[\\*:?{}()+]/g, '\\$&');
    const isRecord = value => value !== null && (typeof value === 'object' || typeof value === 'function');
    const isStringInput = value => value !== null && value !== undefined && !isRecord(value);
    const component = (url, name) => {
        const value = url[name];
        return name === 'protocol' ? value.replace(/:$/, '') :
            name === 'search' ? value.replace(/^\?/, '') :
            name === 'hash' ? value.replace(/^#/, '') : value;
    };
    const absoluteParts = value => parts(resolve(usv(value), undefined));
    const dictionary = value => {
        const result = {};
        if (value == null) return result;
        if (!isRecord(value)) throw new TypeError('URLPattern input must be a string or dictionary');
        for (const name of [...names, 'baseURL']) {
            if (name in value) {
                const member = value[name];
                if (member !== undefined) result[name] = usv(member);
            }
        }
        return result;
    };
    const normalize = (name, value, protocol, mode) => {
        if (name === 'protocol') {
            const stripped = value.replace(/:$/, '');
            if (mode === 'url' && stripped && !/^[A-Za-z][A-Za-z0-9+.-]*$/.test(stripped))
                throw new TypeError('Invalid URLPattern protocol');
            return mode === 'pattern' ? stripped : stripped.toLowerCase();
        }
        if (name === 'hostname') {
            if (!value) return value;
            if (mode === 'url') return absoluteParts('https://' + value + '/').hostname;
            if (!/[\\*:]/.test(value)) return absoluteParts('https://' + value + '/').hostname;
            if (/[^\x00-\x7f]/.test(value))
                throw new TypeError('Non-ASCII dynamic hostname patterns are not supported');
            return value;
        }
        if (name === 'search') return value.replace(/^\?/, '');
        if (name === 'hash') return value.replace(/^#/, '');
        if (name === 'port') {
            if (/^[0-9]+$/.test(value)) {
                const port = Number(value);
                if (port > 65535) throw new TypeError('Invalid URLPattern port');
                value = String(port);
            } else if (mode === 'url' && value || mode === 'pattern' &&
                value && value !== '*' && !/^:[A-Za-z_][A-Za-z_0-9]*\??$/.test(value))
                throw new TypeError('Invalid URLPattern port');
            if (specialPorts[protocol.toLowerCase()] === value) return '';
        }
        if (mode === 'url' && (name === 'username' || name === 'password')) {
            const url = __hostCall('setWebUrlComponent', 'https://urlpattern.invalid/', name, value);
            return parts(url)[name];
        }
        return value;
    };
    const normalizeUrlComponent = (name, value) => {
        if (!value) return value;
        if (name === 'pathname') {
            const safe = value.replace(/\?/g, '%3F').replace(/#/g, '%23');
            const prefix = safe.startsWith('/') ? '' : '/-/';
            const path = absoluteParts('https://urlpattern.invalid' + prefix + safe).pathname;
            return prefix ? path.slice(prefix.length) : path;
        }
        if (name === 'search') return component(absoluteParts(
            'https://urlpattern.invalid/?' + value.replace(/#/g, '%23')), 'search');
        if (name === 'hash') return component(absoluteParts(
            'https://urlpattern.invalid/#' + value), 'hash');
        return value;
    };
    const protectPatternSyntax = value => {
        const tokens = [];
        let marker = 'BREEZEURLPATTERNESC';
        while (value.includes(marker)) marker += 'X';
        let masked = '';
        for (let i = 0; i < value.length; i++) {
            if (value[i] === '\\' || '?{}()+'.includes(value[i])) {
                const token = marker + tokens.length + 'TOKEN';
                tokens.push(value[i] === '\\' && i + 1 < value.length ?
                    value.slice(i, ++i + 1) : value[i]);
                masked += token;
            } else masked += value[i] === '#' ? '%23' : value[i];
        }
        return { masked, restore(serialized) {
            tokens.forEach((original, index) => {
                serialized = serialized.replace(marker + index + 'TOKEN', original);
            });
            return serialized;
        } };
    };
    // URL parsing canonicalizes literal text without consuming pattern syntax.
    const normalizePathPattern = value => {
        if (value === '*') return value;
        const protectedValue = protectPatternSyntax(value);
        const prefix = protectedValue.masked.startsWith('/') ? '' : '/-/';
        let path = absoluteParts('https://urlpattern.invalid' + prefix +
            protectedValue.masked).pathname;
        if (prefix) path = path.slice(prefix.length);
        return protectedValue.restore(path);
    };
    const normalizeSuffixPattern = (name, value) => {
        const protectedValue = protectPatternSyntax(value);
        const delimiter = name === 'search' ? '?' : '#';
        const url = absoluteParts('https://urlpattern.invalid/' + delimiter +
            protectedValue.masked);
        return protectedValue.restore(component(url, name));
    };
    // The URL Pattern Standard has two specificity orders: one for path/query/hash,
    // another for credentials. Pattern credentials never inherit from the base.
    const processInit = (init, mode) => {
        const base = init.baseURL === undefined ? null : absoluteParts(init.baseURL);
        const result = {};
        const has = name => Object.hasOwn(init, name);
        const pathOrder = ['protocol', 'hostname', 'port', 'pathname', 'search', 'hash'];
        const credentialOrder = ['protocol', 'hostname', 'port', 'username', 'password'];
        for (const name of names) {
            const order = name === 'username' || name === 'password' ? credentialOrder : pathOrder;
            const earlier = order.slice(0, order.indexOf(name) + 1);
            const inherit = base && !earlier.some(has) &&
                (mode !== 'pattern' || name !== 'username' && name !== 'password');
            if (has(name)) result[name] = init[name];
            else if (inherit) {
                const inherited = component(base, name);
                result[name] = mode === 'pattern' ? escapePattern(inherited) : inherited;
            }
            else result[name] = mode === 'pattern' ? '*' : '';
        }
        const rawPath = init.pathname;
        if (rawPath !== undefined && base && !/^(?:\/|\{\/)/.test(rawPath)) {
            if (mode === 'pattern') {
                const prefix = base.pathname.slice(0, base.pathname.lastIndexOf('/') + 1);
                result.pathname = prefix + rawPath;
            } else {
                const url = parts(resolve(rawPath, base.href));
                result.pathname = url.pathname;
            }
        }
        if (mode === 'pattern') result.pathname = normalizePathPattern(result.pathname);
        for (const name of names) {
            result[name] = normalize(name, result[name], result.protocol, mode);
            if (mode === 'pattern' && (name === 'search' || name === 'hash') &&
                result[name] !== '*') result[name] = normalizeSuffixPattern(name, result[name]);
            if (mode === 'url') result[name] = normalizeUrlComponent(name, result[name]);
        }
        return result;
    };
    // Constructor-string separators are recognized only when unescaped and not
    // an optional named-part modifier. Regex/group syntax is rejected below.
    const splitSuffix = value => {
        let pathname = value, search, hash, queryAt = -1, hashAt = -1;
        for (let i = 0; i < value.length; i++) {
            if (value[i] === '\\') { i++; continue; }
            if (value[i] === '#' && hashAt < 0) { hashAt = i; break; }
            if (value[i] === '?' && queryAt < 0 &&
                !/:[A-Za-z_][A-Za-z_0-9]*$/.test(value.slice(0, i))) queryAt = i;
        }
        if (hashAt >= 0) { hash = value.slice(hashAt + 1); pathname = value.slice(0, hashAt); }
        if (queryAt >= 0 && (hashAt < 0 || queryAt < hashAt)) {
            search = pathname.slice(queryAt + 1); pathname = pathname.slice(0, queryAt);
        }
        return { pathname, search, hash };
    };
    const parseAuthority = value => {
        const slash = value.search(/[/\\?#]/);
        const authority = slash < 0 ? value : value.slice(0, slash);
        const suffix = slash < 0 ? '' : value.slice(slash);
        if (!authority) throw new TypeError('URLPattern authority is empty');
        let host = authority, username, password;
        const at = authority.lastIndexOf('@');
        if (at >= 0) {
            const credentials = authority.slice(0, at);
            host = authority.slice(at + 1);
            const colon = credentials.indexOf(':');
            username = colon < 0 ? credentials : credentials.slice(0, colon);
            password = colon < 0 ? '' : credentials.slice(colon + 1);
        }
        let hostname = host, port = '';
        if (host.startsWith('[')) {
            const close = host.indexOf(']');
            if (close < 0) throw new TypeError('Invalid URLPattern IPv6 hostname');
            hostname = host.slice(0, close + 1);
            if (host.length > close + 1) {
                if (host[close + 1] !== ':') throw new TypeError('Invalid URLPattern port');
                port = host.slice(close + 2);
            }
        } else {
            const colon = host.lastIndexOf(':');
            if (colon > 0 && /^(?:[0-9]+|\*)$/.test(host.slice(colon + 1))) {
                hostname = host.slice(0, colon); port = host.slice(colon + 1);
            }
        }
        if (!hostname) throw new TypeError('URLPattern hostname is empty');
        return { hostname, port, username, password, suffix };
    };
    const shorthand = (input, baseURL) => {
        const base = baseURL === undefined ? null : absoluteParts(baseURL);
        const scheme = /^([A-Za-z][A-Za-z0-9+.-]*):\/\//.exec(input);
        let init, suffix;
        if (scheme || input.startsWith('//')) {
            const protocol = scheme ? scheme[1] : base && component(base, 'protocol');
            if (!protocol) throw new TypeError('A base URL is required for a relative URLPattern');
            const authority = parseAuthority(input.slice(scheme ? scheme[0].length : 2));
            init = { protocol, hostname: authority.hostname, port: authority.port };
            if (authority.username !== undefined) init.username = authority.username;
            if (authority.password !== undefined) init.password = authority.password;
            suffix = authority.suffix;
        } else {
            if (!base) throw new TypeError('A base URL is required for a relative URLPattern');
            if (/^[A-Za-z][A-Za-z0-9+.-]*:/.test(input))
                throw new TypeError('Opaque-scheme constructor strings are not supported');
            init = { baseURL };
            suffix = input;
        }
        const pieces = splitSuffix(suffix);
        if (pieces.pathname) init.pathname = pieces.pathname;
        else if (pieces.search !== undefined || pieces.hash !== undefined) {
            if (init.baseURL === undefined) init.pathname = '/';
        }
        if (pieces.search !== undefined) init.search = pieces.search;
        else if (pieces.hash !== undefined) init.search = '';
        if (pieces.hash !== undefined) init.hash = pieces.hash;
        return init;
    };
    const compile = (name, input, ignoreCase) => {
        if (input.length > MAX_PATTERN_LENGTH) throw new TypeError('URLPattern component is too long');
        let source = '', pattern = '^', unnamed = 0, wildcardCount = 0;
        let previousDelimiter = '', segmentHasCapture = false;
        const groups = [], used = new Set();
        const appendLiteral = (character, escaped = false) => {
            const value = name === 'protocol' || name === 'hostname' ? character.toLowerCase() : character;
            source += escaped ? '\\' + value : value;
            pattern += escapeRegExp(value);
            previousDelimiter = !escaped && (name === 'pathname' && value === '/' ||
                name === 'hostname' && value === '.') ? value : '';
            if (previousDelimiter) segmentHasCapture = false;
        };
        for (let i = 0; i < input.length; i++) {
            const character = input[i];
            if (character === '\\') {
                if (++i >= input.length) throw new TypeError('Incomplete URLPattern escape');
                appendLiteral(input[i], true); continue;
            }
            if (character === '*') {
                if (++wildcardCount > 1 || name === 'pathname' && i !== input.length - 1 ||
                    segmentHasCapture || name !== 'pathname' && groups.length)
                    throw new TypeError('Ambiguous URLPattern wildcard');
                const key = String(unnamed++);
                groups.push(key); source += '*'; pattern += '(.*)'; previousDelimiter = '';
                segmentHasCapture = true;
                continue;
            }
            if (character === ':' && /[A-Za-z_]/.test(input[i + 1] || '')) {
                let end = i + 2;
                while (end < input.length && /[A-Za-z_0-9]/.test(input[end])) end++;
                const key = input.slice(i + 1, end);
                if (used.has(key)) throw new TypeError('Duplicate URLPattern group name');
                const delimiter = name === 'pathname' ? '/' : name === 'hostname' ? '.' : null;
                const after = end + (input[end] === '?' ? 1 : 0);
                if (delimiter && (segmentHasCapture ||
                    i > 0 && input[i - 1] !== delimiter ||
                    after < input.length && input[after] !== delimiter) ||
                    !delimiter && groups.length || name === 'hostname' && wildcardCount)
                    throw new TypeError('Ambiguous URLPattern capture');
                used.add(key); groups.push(key);
                if (groups.length > MAX_GROUPS) throw new TypeError('Too many URLPattern groups');
                const optional = input[end] === '?';
                const atom = name === 'pathname' ? '[^/]+?' :
                    name === 'hostname' ? '[^.]+?' : '.+?';
                if (optional && previousDelimiter) {
                    pattern = pattern.slice(0, -escapeRegExp(previousDelimiter).length);
                    pattern += '(?:' + escapeRegExp(previousDelimiter) + '(' + atom + '))?';
                } else pattern += '(' + atom + ')' + (optional ? '?' : '');
                source += ':' + key + (optional ? '?' : '');
                i = end - 1 + (optional ? 1 : 0); previousDelimiter = '';
                segmentHasCapture = true;
                continue;
            }
            if ('{}()+?'.includes(character))
                throw new TypeError('Unsupported URLPattern group or modifier syntax');
            appendLiteral(character);
        }
        if (groups.length > MAX_GROUPS) throw new TypeError('Too many URLPattern groups');
        pattern += '$';
        return { source, regexp: new RegExp(pattern, ignoreCase ? 'i' : ''), groups };
    };
    const matchInput = (input, baseURL) => {
        const isURL = input instanceof URLConstructor;
        if (isStringInput(input) || isURL) {
            const text = usv(input);
            const base = baseURL === undefined ? undefined : usv(baseURL);
            try {
                const url = parts(resolve(text, base));
                return { values: Object.fromEntries(names.map(name => [name, component(url, name)])),
                    inputs: base === undefined ? [isURL ? text : input] :
                        [isURL ? text : input, base] };
            } catch (_) { return null; }
        }
        if (baseURL !== undefined) throw new TypeError('A URLPattern dictionary cannot use a second base URL');
        const init = dictionary(input);
        if (Object.values(init).some(value => value.length > MAX_COMPONENT_LENGTH)) return null;
        try { return { values: processInit(init, 'url'), inputs: [input] }; }
        catch (_) { return null; }
    };
    const perform = (receiver, input, baseURL) => {
        const state = slots.get(receiver);
        if (!state) throw new TypeError('Illegal URLPattern invocation');
        const prepared = matchInput(input, baseURL);
        if (!prepared) return null;
        const result = { inputs: prepared.inputs };
        for (const name of names) {
            const value = prepared.values[name];
            if (value.length > MAX_COMPONENT_LENGTH) return null;
            const compiled = state[name];
            const match = compiled.regexp.exec(value);
            if (!match) return null;
            const groups = {};
            compiled.groups.forEach((key, index) =>
                Object.defineProperty(groups, key, { value: match[index + 1],
                    enumerable: true, configurable: true, writable: true }));
            result[name] = { input: value, groups };
        }
        return result;
    };
    class URLPattern {
        constructor(input = {}, baseOrOptions = undefined, options = undefined) {
            const isString = isStringInput(input);
            let init, settings;
            if (isString) {
                const hasBase = isStringInput(baseOrOptions);
                init = shorthand(usv(input), hasBase ? usv(baseOrOptions) : undefined);
                settings = hasBase ? options : baseOrOptions;
            } else {
                if (typeof baseOrOptions === 'string' || options !== undefined)
                    throw new TypeError('A URLPattern dictionary cannot use a second base URL');
                init = dictionary(input); settings = baseOrOptions;
            }
            if (Object.entries(init).some(([name, value]) =>
                name !== 'baseURL' && value.length > MAX_PATTERN_LENGTH))
                throw new TypeError('URLPattern component is too long');
            const ignoreCase = Boolean(settings?.ignoreCase);
            const components = processInit(init, 'pattern');
            const compiled = {};
            for (const name of names) compiled[name] = compile(name, components[name],
                ignoreCase && ['pathname', 'search', 'hash'].includes(name));
            slots.set(this, compiled);
        }
        test(input = {}, baseURL = undefined) { return perform(this, input, baseURL) !== null; }
        exec(input = {}, baseURL = undefined) { return perform(this, input, baseURL); }
        get hasRegExpGroups() { if (!slots.has(this)) throw new TypeError('Illegal URLPattern invocation'); return false; }
    }
    for (const name of names) Object.defineProperty(URLPattern.prototype, name, {
        get() {
            const state = slots.get(this);
            if (!state) throw new TypeError('Illegal URLPattern invocation');
            return state[name].source;
        }, enumerable: true, configurable: true
    });
    Object.defineProperty(URLPattern.prototype, Symbol.toStringTag,
        { value: 'URLPattern', configurable: true });
    for (const name of ['test', 'exec', 'hasRegExpGroups']) {
        const descriptor = Object.getOwnPropertyDescriptor(URLPattern.prototype, name);
        Object.defineProperty(URLPattern.prototype, name, { ...descriptor, enumerable: true });
    }
    Object.defineProperty(globalThis, 'URLPattern',
        { value: URLPattern, configurable: true, writable: true });
})();
