    class DOMTokenList {
        constructor(element, attribute) {
            this.element = element; this.attribute = attribute;
            return new Proxy(this, {
                get(target, property, receiver) {
                    const index = collectionIndex(property);
                    if (index !== null && index < target.length) return target.item(index);
                    return Reflect.get(target, property, receiver);
                },
                has(target, property) {
                    const index = collectionIndex(property);
                    return (index !== null && index < target.length) || Reflect.has(target, property);
                },
                ownKeys(target) {
                    return [...Array(target.length).keys()].map(String).concat(Reflect.ownKeys(target));
                },
                getOwnPropertyDescriptor(target, property) {
                    const index = collectionIndex(property);
                    return index !== null && index < target.length
                        ? { value: target.item(index), writable: false, enumerable: true, configurable: true }
                        : Reflect.getOwnPropertyDescriptor(target, property);
                },
                set(target, property, value, receiver) {
                    return collectionIndex(property) === null && Reflect.set(target, property, value, receiver);
                },
                defineProperty(target, property, descriptor) {
                    return collectionIndex(property) === null && Reflect.defineProperty(target, property, descriptor);
                },
                deleteProperty(target, property) {
                    const index = collectionIndex(property);
                    return !(index !== null && index < target.length) && Reflect.deleteProperty(target, property);
                }
            });
        }
        _tokens() { return [...new Set((this.element.getAttribute(this.attribute) || '').split(/[\t\n\f\r ]+/).filter(Boolean))]; }
        _set(tokens) { this.element.setAttribute(this.attribute, [...new Set(tokens)].join(' ')); }
        _validated(tokens) {
            const strings = tokens.map(String);
            for (const token of strings) {
                if (!token) throw new DOMException('Token must not be empty', 'SyntaxError');
                if (/[\t\n\f\r ]/.test(token))
                    throw new DOMException('Token must not contain ASCII whitespace', 'InvalidCharacterError');
            }
            return strings;
        }
        contains(token) { return this._tokens().includes(String(token)); }
        add(...tokens) { this._set(this._tokens().concat(this._validated(tokens))); }
        remove(...tokens) {
            const remove = new Set(this._validated(tokens));
            this._set(this._tokens().filter(token => !remove.has(token)));
        }
        toggle(token, force) {
            token = this._validated([token])[0];
            if (arguments.length > 1) force = Boolean(force);
            const present = this.contains(token);
            if (present) {
                if (force === undefined || !force) { this.remove(token); return false; }
                return true;
            }
            if (force === undefined || force) { this.add(token); return true; }
            return false;
        }
          replace(oldToken, newToken) {
            [oldToken, newToken] = this._validated([oldToken, newToken]);
            const tokens = this._tokens();
            const index = tokens.indexOf(oldToken);
            if (index < 0) return false;
            tokens[index] = newToken;
            this._set(tokens);
              return true;
          }
          supports(token) {
              if (this.attribute !== 'rel')
                  throw new TypeError('This DOMTokenList has no supported tokens');
              const name = this.element.localName;
              if (name === 'link')
                  return new Set(['stylesheet', 'preload', 'modulepreload', 'dns-prefetch', 'preconnect']).has(String(token).toLowerCase());
              if (name === 'a' || name === 'area')
                  return new Set(['noopener', 'noreferrer']).has(String(token).toLowerCase());
              throw new TypeError('This DOMTokenList has no supported tokens');
          }
        get value() { return this.element.getAttribute(this.attribute) || ''; }
        set value(value) { this.element.setAttribute(this.attribute, value); }
        get length() { return this._tokens().length; }
        item(index) { return this._tokens()[index] || null; }
        toString() { return this.value; }
    }
    installIndexedIterator(DOMTokenList.prototype, true);
