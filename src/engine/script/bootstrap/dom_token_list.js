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
        contains(token) { return this._tokens().includes(String(token)); }
        add(...tokens) { this._set(this._tokens().concat(tokens.map(String))); }
        remove(...tokens) { const remove = new Set(tokens.map(String)); this._set(this._tokens().filter(token => !remove.has(token))); }
        toggle(token, force) {
            token = String(token);
            const present = this.contains(token);
            if (force === true || (!present && force !== false)) { this.add(token); return true; }
            if (present) this.remove(token);
            return false;
        }
          replace(oldToken, newToken) {
            const tokens = this._tokens();
            const index = tokens.indexOf(String(oldToken));
            if (index < 0) return false;
            tokens[index] = String(newToken);
            this._set(tokens);
              return true;
          }
          supports(token) {
              if (this.attribute !== 'rel')
                  throw new TypeError('This DOMTokenList has no supported tokens');
              const name = this.element.localName;
              if (name === 'link')
                  return new Set(['stylesheet', 'preload', 'modulepreload']).has(String(token).toLowerCase());
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
