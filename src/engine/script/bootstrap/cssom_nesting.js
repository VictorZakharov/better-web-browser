    // CSS Nesting Level 1 groups child rules while retaining declaration runs in source order.
    // https://drafts.csswg.org/css-nesting-1/#cssom
    function scanNestedCssItems(source) {
        const items = [];
        let itemStart = 0, declarationsStart = 0;
        let quote = '', escaped = false, comment = false;
        let parentheses = 0, brackets = 0;
        for (let index = 0; index < source.length; index++) {
            const character = source[index], next = source[index + 1];
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
            if (character === '(') { parentheses++; continue; }
            if (character === ')') { parentheses = Math.max(0, parentheses - 1); continue; }
            if (character === '[') { brackets++; continue; }
            if (character === ']') { brackets = Math.max(0, brackets - 1); continue; }
            if (parentheses || brackets) continue;
            if (character === ';') {
                const statement = source.slice(itemStart, index).trim();
                if (statement.startsWith('@')) {
                    if (declarationsStart < itemStart)
                        items.push({kind:'declarations', text:source.slice(declarationsStart, itemStart)});
                    items.push({kind:'rule', text:source.slice(itemStart, index + 1).trim()});
                    declarationsStart = index + 1;
                }
                itemStart = index + 1;
                continue;
            }
            if (character !== '{') continue;
            let depth = 1, innerQuote = '', innerEscaped = false, innerComment = false;
            let close = index + 1;
            for (; close < source.length && depth; close++) {
                const current = source[close], after = source[close + 1];
                if (innerComment) {
                    if (current === '*' && after === '/') { innerComment = false; close++; }
                    continue;
                }
                if (innerQuote) {
                    if (innerEscaped) innerEscaped = false;
                    else if (current === '\\') innerEscaped = true;
                    else if (current === innerQuote) innerQuote = '';
                    continue;
                }
                if (current === '/' && after === '*') { innerComment = true; close++; continue; }
                if (current === '"' || current === "'") { innerQuote = current; continue; }
                if (current === '{') depth++;
                else if (current === '}') depth--;
            }
            if (depth) break;
            const prelude = source.slice(itemStart, index).trim();
            if (/^--[^:]+\s*:/.test(prelude)) { index = close - 1; continue; }
            if (declarationsStart < itemStart)
                items.push({kind:'declarations', text:source.slice(declarationsStart, itemStart)});
            items.push({kind:'rule', text:source.slice(itemStart, close).trim()});
            index = close - 1;
            itemStart = close;
            declarationsStart = close;
        }
        if (declarationsStart < source.length)
            items.push({kind:'declarations', text:source.slice(declarationsStart)});
        return items;
    }

    function parseNestedCssBody(sheet, source, group = false) {
        let leading = '';
        const children = [];
        let beforeFirstRule = !group;
        for (const item of scanNestedCssItems(source)) {
            if (item.kind === 'declarations') {
                if (!splitDeclarations(item.text).length) continue;
                if (beforeFirstRule) leading += item.text;
                else children.push(new CSSNestedDeclarations(
                    sheet, item.text, cssRuleConstructionToken));
                continue;
            }
            beforeFirstRule = false;
            const child = createCssRule(sheet, item.text, true);
            if (!(child instanceof CSSImportRule)) children.push(child);
        }
        return {leading, children};
    }

    class CSSNestedDeclarations extends CSSRule {
        constructor(sheet, text, token) {
            super(sheet, text, token);
            this.__style = ruleStyleProxy(new RuleStyleDeclaration(this, text));
        }
        get style() { return this.__style; }
        get cssText() { return this.__style.cssText; }
        __changed() {
            if (this.parentRule) this.parentRule.__changed();
            else this.parentStyleSheet?.__rulesChanged();
        }
    }

    CSSStyleRule = class CSSStyleRule extends CSSGroupingRule {
        constructor(sheet, text, token) {
            super(sheet, text, token, 'defer');
            const open = text.indexOf('{');
            const close = text.lastIndexOf('}');
            this.__selector = text.slice(0, open).trim();
            const body = parseNestedCssBody(sheet,
                open >= 0 && close > open ? text.slice(open + 1, close) : '');
            this.__style = ruleStyleProxy(new RuleStyleDeclaration(this, body.leading));
            this.__rules.push(...body.children);
            for (const child of this.__rules) child.__parentRule = this;
        }
        get type() { return CSSRule.STYLE_RULE; }
        get selectorText() { return this.__selector; }
        set selectorText(value) { this.__selector = String(value).trim(); this.__changed(); }
        get style() { return this.__style; }
        get cssText() {
            return this.__pristine ? this.__text :
                this.__selector + ' { ' + this.__style.cssText + ' ' +
                this.__serializedBody() + ' }';
        }
        set cssText(value) {
            const parsed = createCssRule(this.parentStyleSheet, String(value), this.__nestedContext);
            if (!(parsed instanceof CSSStyleRule)) return;
            this.__selector = parsed.__selector;
            this.__style = parsed.__style;
            this.__style.__rule = this;
            this.__rules.splice(0, this.__rules.length, ...parsed.__rules);
            for (const child of this.__rules) child.__parentRule = this;
            this.__changed();
        }
    };

    Object.defineProperty(CSSNestedDeclarations.prototype, Symbol.toStringTag,
        {value:'CSSNestedDeclarations', configurable:true});
    windowObject.CSSNestedDeclarations = CSSNestedDeclarations;
