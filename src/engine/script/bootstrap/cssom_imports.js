    // One object per import occurrence, even when network bytes are shared by URL.
    class CSSImportRule extends CSSRule {
        constructor(parent, text, fields, token) {
            super(parent, text, token);
            this.__href = fields[0];
            this.__supports = fields[2];
            this.__layer = fields[3];
            this.__sheet = null;
            this.__media = new MediaList(fields[1], () => this.parentStyleSheet?.__notifyRoots());
        }
        get type() { return 3; }
        get href() { return this.__href; }
        get supportsText() { return this.__supports; }
        get layerName() { return this.__layer; }
        get media() { return this.__media; }
        set media(value) { this.__media.mediaText = value; }
        get cssText() {
            return '@import url("' + this.href.replace(/["\\]/g, '\\$&') + '")' +
                (this.layerName === null ? '' : this.layerName === '' ? ' layer' : ' layer(' + this.layerName + ')') +
                (this.supportsText === null ? '' : ' supports(' + this.supportsText + ')') +
                (this.media.mediaText ? ' ' + this.media.mediaText : '') + ';';
        }
        set cssText(_value) {}
        get styleSheet() {
            if (this.__sheet) return this.__sheet;
            const parent = this.parentStyleSheet;
            if (!parent) return null;
            let url;
            try { url = host('strictResolveUrl', this.href, parent.__baseUrl); }
            catch (_) { return null; }
            let ancestor = parent, depth = 0;
            while (ancestor) {
                if (++depth > 32 || ancestor.href === url) return null;
                ancestor = ancestor.parentStyleSheet;
            }
            if (this.supportsText !== null && !CSS.supports(this.supportsText) &&
                !CSS.supports('(' + this.supportsText + ')')) return null;
            const source = host('stylesheetSource', url);
            if (source === null) return null;
            const sheet = new CSSStyleSheet();
            sheet.__initializing = true;
            sheet.__constructed = false;
            sheet.__constructorDocument = null;
            sheet.__href = url;
            sheet.__baseUrl = host('stylesheetBase', url);
            sheet.__originClean = parent.__originClean && host('stylesheetSameOrigin', url);
            sheet.__ownerRule = this;
            sheet.__media = this.__media;
            sheet.__setText(source);
            sheet.__initializing = false;
            this.__sheet = sheet;
            return sheet;
        }
    }
    for (const [name, value] of Object.entries({STYLE_RULE:1, CHARSET_RULE:2, IMPORT_RULE:3,
        MEDIA_RULE:4, FONT_FACE_RULE:5, PAGE_RULE:6, MARGIN_RULE:9, NAMESPACE_RULE:10})) {
        Object.defineProperty(CSSRule, name, {value, enumerable:true, configurable:true});
        Object.defineProperty(CSSRule.prototype, name, {value, enumerable:true, configurable:true});
    }
    windowObject.CSSImportRule = CSSImportRule;

    function syncOwnedStyleSheet(sheet) {
        while (sheet.parentStyleSheet) sheet = sheet.parentStyleSheet;
        const owner = sheet.ownerNode;
        if (!owner || associatedStyleSheet(owner) !== sheet) return;
        const records = [];
        function visit(current, path) {
            if (records.length >= 257 || path.length >= 32)
                throw new DOMException('Stylesheet occurrence limit exceeded', 'QuotaExceededError');
            records.push([path.join('.'), current.__serialize(), current.media.mediaText, current.disabled]);
            let index = 0;
            for (const rule of current.__rules) {
                if (!(rule instanceof CSSImportRule)) continue;
                // Unmaterialized occurrences retain the native loader's source and conditions.
                if (rule.__sheet) visit(rule.__sheet, path.concat(index));
                else if (rule.styleSheet) visit(rule.__sheet, path.concat(index));
                index++;
            }
        }
        visit(sheet, []);
        host('stylesheetOverrides', owner.__id, records);
    }
