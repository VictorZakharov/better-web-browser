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
        get parentStyleSheet() {
            return this.__parentRule ? this.__parentRule.parentStyleSheet : this.__parentStyleSheet;
        }
        get parentRule() { return this.__parentRule; }
        get type() { return 0; }
    }
    CSSRule.STYLE_RULE = 1;

    // Defined after CSSGroupingRule so CSSStyleRule has the standard grouping inheritance.
    let CSSStyleRule;

    function createCssRule(sheet, text, nestedContext = false) {
        text = text.replace(/^(?:\s|\/\*[\s\S]*?\*\/)+/, '');
        const imported = /^@import\b/i.test(text)
            ? host('stylesheetImport', text) : null;
        if (imported) return new CSSImportRule(sheet, text, imported, cssRuleConstructionToken);
        if (/^@layer(?:\s|\{|;)/i.test(text)) {
            const open = text.indexOf('{');
            const block = open >= 0 && text.trimEnd().endsWith('}');
            const prelude = (block ? text.slice(0, open) : text.slice(0, text.lastIndexOf(';'))).trim();
            const names = host('stylesheetLayerNames', prelude, block);
            if (names === null || (!block && !text.trimEnd().endsWith(';')))
                throw new DOMException('Invalid @layer rule', 'SyntaxError');
            return block
                ? new CSSLayerBlockRule(sheet, text, names[0] || '', cssRuleConstructionToken,
                    nestedContext)
                : new CSSLayerStatementRule(sheet, text, names, cssRuleConstructionToken);
        }
        const open = text.indexOf('{');
        if (open >= 0 && text.trimEnd().endsWith('}')) {
            if (/^@media\s/i.test(text))
                return new CSSMediaRule(sheet, text, text.slice(6, open).trim(),
                    cssRuleConstructionToken, nestedContext);
            if (/^@supports\s/i.test(text))
                return new CSSSupportsRule(sheet, text, text.slice(9, open).trim(),
                    cssRuleConstructionToken, nestedContext);
        }
        return open > 0 && !text.trimStart().startsWith('@')
            ? new CSSStyleRule(sheet, text, cssRuleConstructionToken)
            : new CSSRule(sheet, text, cssRuleConstructionToken);
    }

    function parseCssRules(sheet, text) {
        let importsAllowed = true;
        let sawImport = false;
        const rules = [];
        for (const source of scanCssRules(text)) {
            const text = source.replace(/^(?:\s|\/\*[\s\S]*?\*\/)+/, '');
            if (/^@charset\b/i.test(text)) continue;
            if (/^@import\b/i.test(text)) {
                if (sheet.__constructed || !importsAllowed) continue;
                const rule = createCssRule(sheet, text);
                if (rule instanceof CSSImportRule) { rules.push(rule); sawImport = true; }
            } else {
                let rule;
                try { rule = createCssRule(sheet, text); }
                catch (error) {
                    if (error?.name === 'SyntaxError') continue;
                    throw error;
                }
                if (!(rule instanceof CSSLayerStatementRule) || sawImport)
                    importsAllowed = false;
                rules.push(rule);
            }
        }
        return rules;
    }
