    // CSS Cascade 5 layer rules are live CSSOM objects, not opaque at-rule text.
    // https://drafts.csswg.org/css-cascade-5/#layer-apis
    class CSSGroupingRule extends CSSRule {
        constructor(sheet, text, token, mode = 'normal') {
            super(sheet, text, token);
            const open = cssRuleBlockStart(text);
            const close = text.lastIndexOf('}');
            this.__nestedContext = mode !== 'normal';
            this.__rules = (mode === 'defer' ? [] : mode === 'nested'
                ? parseNestedCssBody(sheet, text.slice(open + 1, close), true).children
                : parseCssRules(sheet, text.slice(open + 1, close)))
                .filter(rule => !(rule instanceof CSSImportRule));
            this.__ruleList = readonlyRuleList(this.__rules);
            this.__pristine = true;
            for (const rule of this.__rules) rule.__parentRule = this;
        }
        get cssRules() { return this.__ruleList; }
        insertRule(rule, index = 0) {
            index = Number(index) >>> 0;
            if (index > this.__rules.length)
                throw new DOMException('Rule index is outside the list', 'IndexSizeError');
            const source = String(rule);
            const parsed = scanCssRules(source);
            let next = parsed.length === 1
                ? createCssRule(this.parentStyleSheet, parsed[0], this.__nestedContext)
                : null;
            // CSSOM's nested insertion flag permits a declaration block when
            // parsing a single rule fails. Unlike a normal rule, that block can
            // contain several declarations separated by semicolons.
            // https://drafts.csswg.org/cssom/#insert-a-css-rule
            if (this.__nestedContext && (!next || next.constructor === CSSRule) &&
                splitDeclarations(source).length > 0)
                next = new CSSNestedDeclarations(this.parentStyleSheet, source,
                    cssRuleConstructionToken);
            if (!next || (next.constructor === CSSRule &&
                !source.trimStart().startsWith('@')))
                throw new DOMException('Expected one valid CSS rule', 'SyntaxError');
            if (next instanceof CSSImportRule)
                throw new DOMException('@import is not allowed in a grouping rule', 'HierarchyRequestError');
            next.__parentRule = this;
            this.__rules.splice(index, 0, next);
            this.__changed();
            return index;
        }
        deleteRule(index) {
            index = Number(index) >>> 0;
            if (index >= this.__rules.length)
                throw new DOMException('Rule index is outside the list', 'IndexSizeError');
            const [removed] = this.__rules.splice(index, 1);
            removed.__parentRule = null;
            removed.__parentStyleSheet = null;
            this.__changed();
        }
        __changed() {
            this.__pristine = false;
            if (this.parentRule) this.parentRule.__changed();
            else this.parentStyleSheet?.__rulesChanged();
        }
        __serializedBody() { return this.__rules.map(rule => rule.cssText).join(' '); }
    }

    class CSSLayerBlockRule extends CSSGroupingRule {
        constructor(sheet, text, name, token, nestedContext = false) {
            super(sheet, text, token, nestedContext ? 'nested' : 'normal');
            this.__name = name;
            this.__prelude = text.slice(0, cssRuleBlockStart(text)).trim();
        }
        get name() { return this.__name; }
        get type() { return 0; }
        get cssText() {
            return this.__pristine ? this.__text :
                this.__prelude + ' { ' +
                this.__serializedBody() + ' }';
        }
    }

    class CSSConditionRule extends CSSGroupingRule {
        get conditionText() { return this.__conditionText; }
    }

    function serializeGroupedRule(prelude, rules) {
        const body = rules.map(rule => rule.cssText).filter(Boolean)
            .map(text => '  ' + text.replace(/\n/g, '\n  ')).join('\n');
        return prelude + ' {\n' + (body ? body + '\n' : '') + '}';
    }

    function ruleHasAttachedSheet(rule) {
        let sheet = rule.parentStyleSheet;
        while (sheet?.parentStyleSheet) sheet = sheet.parentStyleSheet;
        if (!sheet) return false;
        if (sheet.ownerNode?.isConnected) return true;
        for (const root of sheet.__adopters) {
            if (root instanceof Document || root.host?.isConnected) return true;
        }
        return false;
    }

    class CSSMediaRule extends CSSConditionRule {
        constructor(sheet, text, condition, token, nestedContext = false) {
            super(sheet, text, token, nestedContext ? 'nested' : 'normal');
            this.__media = new MediaList(condition, () => this.__changed(), mediaListToken);
        }
        get type() { return CSSRule.MEDIA_RULE; }
        get conditionText() { return this.__media.mediaText; }
        get media() { return this.__media; }
        set media(value) { this.__media.mediaText = value; }
        get matches() {
            return ruleHasAttachedSheet(this) && !!host('mediaMatches', this.conditionText);
        }
        get cssText() {
            return serializeGroupedRule('@media ' + this.conditionText, this.__rules);
        }
    }

    class CSSSupportsRule extends CSSConditionRule {
        constructor(sheet, text, condition, token, nestedContext = false) {
            super(sheet, text, token, nestedContext ? 'nested' : 'normal');
            this.__conditionText = condition;
        }
        get type() { return CSSRule.SUPPORTS_RULE; }
        get matches() { return !!host('cssSupports', this.conditionText); }
        get cssText() {
            return this.__pristine ? this.__text :
                '@supports ' + this.conditionText + ' { ' + this.__serializedBody() + ' }';
        }
    }

    // CSS Cascade 6 exposes a scope's boundary selector lists as nullable,
    // readonly strings. The scope prelude is retained for mutation-driven
    // serialization; changing children does not rewrite its boundary grammar.
    // https://drafts.csswg.org/css-cascade-6/#cssscoperule
    class CSSScopeRule extends CSSGroupingRule {
        constructor(sheet, text, boundaries, token) {
            // Runs of declarations in @scope are nested-declarations child rules,
            // including a run before the first style rule.
            super(sheet, text, token, 'nested');
            this.__start = boundaries[0];
            this.__end = boundaries[1];
        }
        get start() { return this.__start; }
        get end() { return this.__end; }
        get cssText() {
            const start = this.start === null ? '' : ' (' + this.start + ')';
            const end = this.end === null ? '' : ' to (' + this.end + ')';
            return serializeGroupedRule('@scope' + start + end, this.__rules);
        }
    }

    class CSSLayerStatementRule extends CSSRule {
        constructor(sheet, text, names, token) {
            super(sheet, text, token);
            this.__nameList = Object.freeze(names.slice());
        }
        get nameList() { return this.__nameList; }
        get type() { return 0; }
    }

    Object.defineProperty(CSSRule, 'SUPPORTS_RULE', {value:12, enumerable:true});
    Object.defineProperty(CSSRule.prototype, 'SUPPORTS_RULE', {value:12, enumerable:true});
    Object.defineProperty(CSSGroupingRule.prototype, Symbol.toStringTag,
        {value:'CSSGroupingRule', configurable:true});
    Object.defineProperty(CSSLayerBlockRule.prototype, Symbol.toStringTag,
        {value:'CSSLayerBlockRule', configurable:true});
    Object.defineProperty(CSSLayerStatementRule.prototype, Symbol.toStringTag,
        {value:'CSSLayerStatementRule', configurable:true});
    Object.defineProperty(CSSConditionRule.prototype, Symbol.toStringTag,
        {value:'CSSConditionRule', configurable:true});
    Object.defineProperty(CSSMediaRule.prototype, Symbol.toStringTag,
        {value:'CSSMediaRule', configurable:true});
    Object.defineProperty(CSSSupportsRule.prototype, Symbol.toStringTag,
        {value:'CSSSupportsRule', configurable:true});
    Object.defineProperty(CSSScopeRule.prototype, Symbol.toStringTag,
        {value:'CSSScopeRule', configurable:true});
    windowObject.CSSGroupingRule = CSSGroupingRule;
    windowObject.CSSLayerBlockRule = CSSLayerBlockRule;
    windowObject.CSSLayerStatementRule = CSSLayerStatementRule;
    windowObject.CSSConditionRule = CSSConditionRule;
    windowObject.CSSMediaRule = CSSMediaRule;
    windowObject.CSSSupportsRule = CSSSupportsRule;
    windowObject.CSSScopeRule = CSSScopeRule;
