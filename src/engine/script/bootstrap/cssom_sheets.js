    class MediaList {
        constructor(text = '', changed = () => {}) {
            this.__changed = changed;
            this.__items = [];
            this.mediaText = text;
        }
        get mediaText() { return this.__items.join(', '); }
        set mediaText(value) {
            this.__items = String(value || '').split(',').map(item => item.trim()).filter(Boolean);
            this.__changed();
        }
        get length() { return this.__items.length; }
        item(index) { return this.__items[Number(index)] ?? null; }
        appendMedium(value) {
            value = String(value).trim();
            if (value && !this.__items.includes(value)) { this.__items.push(value); this.__changed(); }
        }
        deleteMedium(value) {
            const index = this.__items.indexOf(String(value).trim());
            if (index < 0) throw new DOMException('Media query was not found', 'NotFoundError');
            this.__items.splice(index, 1);
            this.__changed();
        }
        toString() { return this.mediaText; }
        [Symbol.iterator]() { return this.__items[Symbol.iterator](); }
    }

    const styleSheetConstructionToken = {};
    class StyleSheet {
        constructor(token) {
            if (token !== styleSheetConstructionToken) throw new TypeError('Illegal constructor');
        }
    }

    function readonlyRuleList(backing) {
        return new Proxy(Object.create(null), {
            get(_target, property) {
                if (property === 'length') return backing.length;
                if (property === 'item') return index => backing[Number(index)] || null;
                if (property === Symbol.iterator) return backing[Symbol.iterator].bind(backing);
                if (cssIndex(property)) return backing[Number(property)];
                return undefined;
            },
            ownKeys() { return backing.map((_rule, index) => String(index)); },
            getOwnPropertyDescriptor(_target, property) {
                if (cssIndex(property) && Number(property) < backing.length)
                    return { configurable: true, enumerable: true, value: backing[Number(property)] };
            }
        });
    }

    class CSSStyleSheet extends StyleSheet {
        constructor(options = {}) {
            super(styleSheetConstructionToken);
            options = Object(options || {});
            this.__constructorDocument = document;
            this.__constructed = true;
            this.__baseUrl = options.baseURL == null ? document.baseURI :
                host('strictResolveUrl', String(options.baseURL), document.baseURI);
            this.__href = null;
            this.__ownerNode = null;
            this.__title = null;
            this.__originClean = true;
            this.__disabled = !!options.disabled;
            this.__rules = [];
            this.__ruleList = readonlyRuleList(this.__rules);
            this.__adopters = new Set();
            this.__modifying = false;
            this.__initializing = false;
            this.__ownerRule = null;
            this.__media = new MediaList(options.media || '', () => this.__notifyRoots());
        }
        get type() { return 'text/css'; }
        get href() { return this.__href; }
        get ownerNode() {
            if (this.__ownerNode && associatedStyleSheet(this.__ownerNode) !== this)
                this.__ownerNode = null;
            return this.__ownerNode;
        }
        get parentStyleSheet() { return this.__ownerRule?.parentStyleSheet || null; }
        get title() { return this.__title; }
        get ownerRule() { return this.__ownerRule; }
        get media() { return this.__media; }
        set media(value) { this.__media.mediaText = value; }
        get disabled() {
            const owner = this.ownerNode;
            return owner ? !!host('stylesheetDisabled', nodeId(owner)) : this.__disabled;
        }
        set disabled(value) {
            value = !!value;
            const owner = this.ownerNode;
            const changed = value !== this.__disabled;
            this.__disabled = value;
            if (owner) host('stylesheetDisable', nodeId(owner), value);
            else if (changed) this.__notifyRoots();
        }
        get cssRules() { this.__assertOriginClean(); return this.__ruleList; }
        get rules() { this.__assertOriginClean(); return this.__ruleList; }
        insertRule(rule, index = 0) {
            this.__assertOriginClean();
            index = Number(index) >>> 0;
            if (index > this.__rules.length)
                throw new DOMException('Rule index is outside the list', 'IndexSizeError');
            const source = String(rule);
            if (this.__constructed && /^\s*@import(?:\s|url\(|['"])/i.test(source))
                throw new DOMException('@import is not allowed in constructed sheets', 'SyntaxError');
            const parsed = scanCssRules(source);
            if (parsed.length !== 1)
                throw new DOMException('Expected exactly one CSS rule', 'SyntaxError');
            const next = createCssRule(this, parsed[0]);
            if (this.__constructed && next instanceof CSSImportRule)
                throw new DOMException('@import is not allowed in constructed sheets', 'SyntaxError');
            const preceding = this.__rules.slice(0, index);
            const following = this.__rules.slice(index);
            const lastImport = preceding.findLastIndex(r => r instanceof CSSImportRule);
            if ((next instanceof CSSImportRule &&
                    (preceding.some(r => !(r instanceof CSSImportRule || r instanceof CSSLayerStatementRule)) ||
                     (lastImport >= 0 && preceding.slice(lastImport + 1)
                         .some(r => r instanceof CSSLayerStatementRule)) ||
                     following.some(r => r instanceof CSSLayerStatementRule))) ||
                (next instanceof CSSLayerStatementRule &&
                    preceding.some(r => r instanceof CSSImportRule) &&
                    following.some(r => r instanceof CSSImportRule)) ||
                (!(next instanceof CSSImportRule || next instanceof CSSLayerStatementRule) &&
                    following.some(r => r instanceof CSSImportRule)))
                throw new DOMException('Invalid ordering of @import and @layer', 'HierarchyRequestError');
            this.__rules.splice(index, 0, next);
            this.__rulesChanged();
            return index;
        }
        deleteRule(index) {
            this.__assertOriginClean();
            index = Number(index) >>> 0;
            if (index >= this.__rules.length)
                throw new DOMException('Rule index is outside the list', 'IndexSizeError');
            const [removed] = this.__rules.splice(index, 1);
            removed.__parentStyleSheet = null;
            removed.__parentRule = null;
            this.__rulesChanged();
        }
        replaceSync(text) {
            this.__assertConstructed();
            if (this.__modifying)
                throw new DOMException('Stylesheet replacement is already active', 'NotAllowedError');
            this.__setText(text);
        }
        replace(text) {
            if (!this.__constructed)
                return Promise.reject(new DOMException(
                    'Only constructed stylesheets can be replaced', 'NotAllowedError'));
            if (this.__modifying)
                return Promise.reject(new DOMException(
                    'Stylesheet replacement is already active', 'NotAllowedError'));
            this.__modifying = true;
            return Promise.resolve().then(() => {
                try {
                    this.__setText(text);
                    return this;
                } finally {
                    this.__modifying = false;
                }
            });
        }
        __setText(text) {
            const rules = parseCssRules(this, String(text));
            this.__rules.splice(0, this.__rules.length, ...rules);
            this.__notifyRoots();
        }
        __setOwner(ownerNode, href, title, media, originClean, text) {
            this.__initializing = true;
            this.__constructed = false;
            this.__constructorDocument = null;
            this.__ownerNode = ownerNode;
            this.__href = href;
            this.__title = ownerNode.getRootNode() instanceof Document ? title || null : null;
            this.__baseUrl = href ? host('stylesheetBase', href) : ownerNode.ownerDocument.baseURI;
            this.__originClean = !!originClean;
            this.__disabled = ownerNode.hasAttribute('disabled');
            this.__media.mediaText = media || '';
            this.__setText(text);
            this.__initializing = false;
        }
        __assertOriginClean() {
            if (!this.__originClean)
                throw new DOMException('Stylesheet rules are not accessible across origins',
                    'SecurityError');
        }
        __assertConstructed() {
            if (!this.__constructed)
                throw new DOMException('Only constructed stylesheets can be replaced',
                    'NotAllowedError');
        }
        __serialize() { return this.__rules.map(rule => rule.cssText).join('\n'); }
        __rulesChanged() { this.__notifyRoots(); }
        __notifyRoots() {
            if (this.__initializing) return;
            if (this.__ownerNode || this.__ownerRule) syncOwnedStyleSheet(this);
            for (const root of [...this.__adopters]) adoptedRecord(root).sync();
        }
    }
