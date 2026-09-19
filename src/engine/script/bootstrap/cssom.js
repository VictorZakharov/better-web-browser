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

    const ruleStyleProxy = declaration => declarationProxy(declaration);

    class CSSRule {
        constructor(sheet, text, token = null) {
            if (token !== cssRuleConstructionToken) throw new TypeError('Illegal constructor');
            this.__parentStyleSheet = sheet;
            this.__parentRule = null;
            this.__text = text.trim();
        }
        get cssText() { return this.__text; }
        set cssText(_value) {}
        get parentStyleSheet() { return this.__parentStyleSheet; }
        get parentRule() { return this.__parentRule; }
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
        __changed() { this.__pristine = false; this.parentStyleSheet?.__rulesChanged(); }
    }

    function createCssRule(sheet, text) {
        const imported = /^@import\b/i.test(text.replace(/^(?:\s|\/\*[\s\S]*?\*\/)+/, ''))
            ? host('stylesheetImport', text) : null;
        if (imported) return new CSSImportRule(sheet, text, imported, cssRuleConstructionToken);
        const open = text.indexOf('{');
        return open > 0 && !text.trimStart().startsWith('@')
            ? new CSSStyleRule(sheet, text, cssRuleConstructionToken)
            : new CSSRule(sheet, text, cssRuleConstructionToken);
    }

    function parseCssRules(sheet, text) {
        let importsAllowed = true;
        const rules = [];
        for (const source of scanCssRules(text)) {
            const text = source.replace(/^(?:\s|\/\*[\s\S]*?\*\/)+/, '');
            if (/^@charset\b/i.test(text)) continue;
            if (/^@import\b/i.test(text)) {
                if (sheet.__constructed || !importsAllowed) continue;
                const rule = createCssRule(sheet, text);
                if (rule instanceof CSSImportRule) rules.push(rule);
            } else {
                if (!/^@layer\b[^{}]*;/i.test(text)) importsAllowed = false;
                rules.push(createCssRule(sheet, text));
            }
        }
        return rules;
    }
