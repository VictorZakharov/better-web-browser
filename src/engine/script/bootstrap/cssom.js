    // Constructed sheets are JavaScript-identity objects whose active sources are snapshotted on
    // their adopting roots. The native cascade therefore owns no cross-realm object references.
    // https://drafts.csswg.org/cssom/#dom-documentorshadowroot-adoptedstylesheets
    const cssRuleConstructionToken = {};
    const cssIndex = property => typeof property === 'string' && /^(0|[1-9][0-9]*)$/.test(property);
    const cssName = property => String(property).replace(/[A-Z]/g, match => '-' + match.toLowerCase());
    const declarationName = name => {
        name = String(name);
        return name.startsWith('--') ? name : name.toLowerCase();
    };

    function scanCssRules(source) {
        source = String(source);
        const rules = [];
        let start = 0;
        let depth = 0;
        let quote = '';
        let comment = false;
        let escaped = false;
        let sawBlock = false;
        for (let index = 0; index < source.length; index++) {
            const character = source[index];
            const next = source[index + 1];
            if (comment) {
                if (character === '*' && next === '/') { comment = false; index++; }
                continue;
            }
            if (quote) {
                if (escaped) escaped = false;
                else if (character === '\\') escaped = true;
                else if (character === quote) quote = '';
                continue;
            }
            if (character === '/' && next === '*') { comment = true; index++; continue; }
            if (character === '"' || character === "'") { quote = character; continue; }
            if (character === '{') { depth++; sawBlock = true; continue; }
            if (character === '}' && depth > 0) {
                depth--;
                if (depth === 0 && sawBlock) {
                    const rule = source.slice(start, index + 1).trim();
                    if (rule) rules.push(rule);
                    start = index + 1;
                    sawBlock = false;
                }
                continue;
            }
            if (character === ';' && depth === 0) {
                const rule = source.slice(start, index + 1).trim();
                if (rule) rules.push(rule);
                start = index + 1;
            }
        }
        return rules;
    }

    function splitDeclarations(source) {
        const declarations = [];
        let start = 0;
        let depth = 0;
        let quote = '';
        let escaped = false;
        for (let index = 0; index <= source.length; index++) {
            const character = source[index] || ';';
            if (quote) {
                if (escaped) escaped = false;
                else if (character === '\\') escaped = true;
                else if (character === quote) quote = '';
                continue;
            }
            if (character === '"' || character === "'") { quote = character; continue; }
            if (character === '(' || character === '[') depth++;
            else if ((character === ')' || character === ']') && depth > 0) depth--;
            else if (character === ';' && depth === 0) {
                const declaration = source.slice(start, index).trim();
                const colon = declaration.indexOf(':');
                if (colon > 0) {
                    const name = declarationName(declaration.slice(0, colon).trim());
                    let value = declaration.slice(colon + 1).trim();
                    const important = /!\s*important\s*$/i.test(value);
                    if (important) value = value.replace(/!\s*important\s*$/i, '').trim();
                    declarations.push([name, value, important ? 'important' : '']);
                }
                start = index + 1;
            }
        }
        return declarations;
    }

    class RuleStyleDeclaration extends CSSStyleDeclaration {
        constructor(rule, source) {
            super(null);
            this.__rule = rule;
            this.__declarations = new Map();
            for (const [name, value, priority] of splitDeclarations(source))
                this.__declarations.set(name, { value, priority });
        }
        get cssText() {
            return [...this.__declarations].map(([name, entry]) =>
                name + ': ' + entry.value + (entry.priority ? ' !important' : '') + ';').join(' ');
        }
        set cssText(value) {
            this.__declarations.clear();
            for (const [name, text, priority] of splitDeclarations(String(value)))
                this.__declarations.set(name, { value: text, priority });
            this.__rule.__changed();
        }
        get length() { return this.__declarations.size; }
        item(index) { return [...this.__declarations.keys()][Number(index)] || ''; }
        getPropertyValue(name) {
            return this.__declarations.get(declarationName(name))?.value || '';
        }
        getPropertyPriority(name) {
            return this.__declarations.get(declarationName(name))?.priority || '';
        }
        setProperty(name, value, priority = '') {
            name = declarationName(name);
            value = String(value);
            priority = String(priority).toLowerCase();
            if (priority && priority !== 'important') return;
            if (!value) { this.removeProperty(name); return; }
            this.__declarations.set(name, { value, priority });
            this.__rule.__changed();
        }
        removeProperty(name) {
            name = declarationName(name);
            const previous = this.getPropertyValue(name);
            if (this.__declarations.delete(name)) this.__rule.__changed();
            return previous;
        }
    }

    const ruleStyleProxy = declaration => new Proxy(declaration, {
        get(target, property) {
            if (property in target) {
                const value = target[property];
                return typeof value === 'function' ? value.bind(target) : value;
            }
            return target.getPropertyValue(cssName(property));
        },
        set(target, property, value) {
            if (property === 'cssText') target.cssText = value;
            else if (property in target) target[property] = value;
            else target.setProperty(cssName(property), value);
            return true;
        }
    });

    class CSSRule {
        constructor(sheet, text, token = null) {
            if (token !== cssRuleConstructionToken) throw new TypeError('Illegal constructor');
            this.parentStyleSheet = sheet;
            this.parentRule = null;
            this.__text = text.trim();
        }
        get cssText() { return this.__text; }
        set cssText(value) { this.__text = String(value).trim(); this.parentStyleSheet.__rulesChanged(); }
        get type() { return 0; }
    }
    CSSRule.STYLE_RULE = 1;

    class CSSStyleRule extends CSSRule {
        constructor(sheet, text, token) {
            super(sheet, text, token);
            const open = text.indexOf('{');
            const close = text.lastIndexOf('}');
            this.__selector = text.slice(0, open).trim();
            this.__style = ruleStyleProxy(new RuleStyleDeclaration(
                this, open >= 0 && close > open ? text.slice(open + 1, close) : ''
            ));
            this.__pristine = true;
        }
        get type() { return CSSRule.STYLE_RULE; }
        get selectorText() { return this.__selector; }
        set selectorText(value) { this.__selector = String(value).trim(); this.__changed(); }
        get style() { return this.__style; }
        get cssText() {
            return this.__pristine ? this.__text :
                this.__selector + ' { ' + this.__style.cssText + ' }';
        }
        set cssText(value) {
            const parsed = createCssRule(this.parentStyleSheet, String(value));
            if (!(parsed instanceof CSSStyleRule)) return;
            this.__selector = parsed.__selector;
            this.__style = parsed.__style;
            this.__style.__rule = this;
            this.__pristine = false;
            this.parentStyleSheet.__rulesChanged();
        }
        __changed() { this.__pristine = false; this.parentStyleSheet.__rulesChanged(); }
    }

    function createCssRule(sheet, text) {
        const open = text.indexOf('{');
        return open > 0 && !text.trimStart().startsWith('@')
            ? new CSSStyleRule(sheet, text, cssRuleConstructionToken)
            : new CSSRule(sheet, text, cssRuleConstructionToken);
    }

    function parseCssRules(sheet, text) {
        return scanCssRules(text)
            .filter(rule => !/^@import(?:\s|url\(|['"])/i.test(
                rule.replace(/^(?:\s|\/\*[\s\S]*?\*\/)+/, '')))
            .map(rule => createCssRule(sheet, rule));
    }

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
            this.__media = new MediaList(options.media || '', () => this.__notifyRoots());
        }
        get type() { return 'text/css'; }
        get href() { return this.__href; }
        get ownerNode() { return this.__ownerNode; }
        get parentStyleSheet() { return null; }
        get title() { return this.__title; }
        get ownerRule() { return null; }
        get media() { return this.__media; }
        set media(value) { this.__media.mediaText = value; }
        get disabled() { return this.__disabled; }
        set disabled(value) {
            value = !!value;
            if (value !== this.__disabled) { this.__disabled = value; this.__notifyRoots(); }
        }
        get cssRules() { this.__assertOriginClean(); return this.__ruleList; }
        get rules() { this.__assertOriginClean(); return this.__ruleList; }
        insertRule(rule, index = 0) {
            this.__assertOriginClean();
            index = Number(index) >>> 0;
            if (index > this.__rules.length)
                throw new DOMException('Rule index is outside the list', 'IndexSizeError');
            const source = String(rule);
            if (/^\s*@import(?:\s|url\(|['"])/i.test(source))
                throw new DOMException('@import is not allowed in constructed sheets', 'SyntaxError');
            const parsed = scanCssRules(source);
            if (parsed.length !== 1)
                throw new DOMException('Expected exactly one CSS rule', 'SyntaxError');
            this.__rules.splice(index, 0, createCssRule(this, parsed[0]));
            this.__rulesChanged();
            return index;
        }
        deleteRule(index) {
            this.__assertOriginClean();
            index = Number(index) >>> 0;
            if (index >= this.__rules.length)
                throw new DOMException('Rule index is outside the list', 'IndexSizeError');
            this.__rules.splice(index, 1);
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
            this.__constructed = false;
            this.__constructorDocument = null;
            this.__ownerNode = ownerNode;
            this.__href = href;
            this.__title = title || null;
            this.__baseUrl = href || ownerNode.ownerDocument.baseURI;
            this.__originClean = !!originClean;
            this.__disabled = ownerNode.hasAttribute('disabled');
            this.__media.mediaText = media || '';
            this.__setText(text);
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
            for (const root of [...this.__adopters]) adoptedRecord(root).sync();
        }
    }
