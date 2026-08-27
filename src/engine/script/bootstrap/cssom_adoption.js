    // ObservableArray mutations keep the root snapshot and sheet adopter sets in lockstep.
    const adoptedStyleSheetRecords = new WeakMap();
    function validateAdoptedSheet(root, sheet) {
        if (!(sheet instanceof CSSStyleSheet)) throw new TypeError('Expected a CSSStyleSheet');
        const rootDocument = root instanceof Document ? root : root.ownerDocument;
        if (sheet.__constructorDocument !== rootDocument)
            throw new DOMException('Stylesheet was constructed for another document', 'NotAllowedError');
    }
    function adoptedRecord(root) {
        let record = adoptedStyleSheetRecords.get(root);
        if (record) return record;
        const backing = [];
        record = {
            backing,
            sheets: new Set(),
            sync() {
                const active = backing.filter(sheet => sheet instanceof CSSStyleSheet);
                const next = new Set(active);
                for (const sheet of this.sheets) if (!next.has(sheet)) sheet.__adopters.delete(root);
                for (const sheet of next) sheet.__adopters.add(root);
                this.sheets = next;
                host('adoptedStyleSheetsSet', root.__id, JSON.stringify(
                    active.filter(sheet => !sheet.disabled).map(sheet => ({
                        baseUrl: sheet.__baseUrl,
                        media: sheet.media.mediaText,
                        source: sheet.__serialize()
                    }))
                ));
            }
        };
        const mutate = (name, argumentsList, validateFrom = 0) => {
            for (let index = validateFrom; index < argumentsList.length; index++)
                validateAdoptedSheet(root, argumentsList[index]);
            const result = Array.prototype[name].apply(backing, argumentsList);
            record.sync();
            return result === backing ? record.proxy : result;
        };
        record.proxy = new Proxy(backing, {
            get(target, property) {
                if (property === 'push' || property === 'unshift')
                    return (...items) => mutate(property, items);
                if (property === 'splice')
                    return (...items) => mutate(property, items, 2);
                if (property === 'fill')
                    return (sheet, ...range) => {
                        validateAdoptedSheet(root, sheet);
                        return mutate(property, [sheet, ...range], range.length + 1);
                    };
                if (['pop', 'shift', 'reverse', 'sort', 'copyWithin'].includes(property))
                    return (...items) => mutate(property, items, items.length);
                const value = Reflect.get(target, property, target);
                return typeof value === 'function' ? value.bind(target) : value;
            },
            set(target, property, value) {
                let changed;
                if (cssIndex(property)) {
                    validateAdoptedSheet(root, value);
                    Object.defineProperty(target, property, {
                        configurable: true, enumerable: true, writable: true, value
                    });
                    changed = true;
                } else {
                    changed = Reflect.set(target, property, value, target);
                }
                if (changed && (cssIndex(property) || property === 'length')) record.sync();
                return changed;
            },
            deleteProperty(target, property) {
                const changed = Reflect.deleteProperty(target, property);
                if (changed && cssIndex(property)) record.sync();
                return changed;
            },
            defineProperty(target, property, descriptor) {
                if (cssIndex(property) && 'value' in descriptor)
                    validateAdoptedSheet(root, descriptor.value);
                const changed = Reflect.defineProperty(target, property, descriptor);
                if (changed && (cssIndex(property) || property === 'length')) record.sync();
                return changed;
            }
        });
        adoptedStyleSheetRecords.set(root, record);
        return record;
    }
    function installAdoptedStyleSheets(prototype) {
        Object.defineProperty(prototype, 'adoptedStyleSheets', {
            configurable: true,
            enumerable: true,
            get() { return adoptedRecord(this).proxy; },
            set(value) {
                const items = Array.from(value);
                for (const sheet of items) validateAdoptedSheet(this, sheet);
                const record = adoptedRecord(this);
                record.backing.splice(0, record.backing.length, ...items);
                record.sync();
            }
        });
    }
    installAdoptedStyleSheets(Document.prototype);
    installAdoptedStyleSheets(ShadowRoot.prototype);

    Object.defineProperty(CSSStyleSheet.prototype, Symbol.toStringTag,
        { value: 'CSSStyleSheet', configurable: true });
    Object.defineProperty(CSSRule.prototype, Symbol.toStringTag,
        { value: 'CSSRule', configurable: true });
    Object.defineProperty(CSSStyleRule.prototype, Symbol.toStringTag,
        { value: 'CSSStyleRule', configurable: true });
    windowObject.StyleSheet = StyleSheet;
    windowObject.CSSStyleSheet = CSSStyleSheet;
    windowObject.CSSRule = CSSRule;
    windowObject.CSSStyleRule = CSSStyleRule;
    windowObject.MediaList = MediaList;
