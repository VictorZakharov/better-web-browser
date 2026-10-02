    // CSSKeyframesRule is not a grouping rule: it has append/find/deleteRule but no insertRule.
    // https://drafts.csswg.org/css-animations-1/#interface-csskeyframesrule
    const cssKeyframeText = offsets => offsets.map(value => `${value * 100}%`).join(', ');
    const cssKeyframesRuleToken = {};
    class KeyframeStyleDeclaration extends RuleStyleDeclaration {
        constructor(rule, source) {
            super(rule, '');
            for (const [name, value] of host('cssDeclarationList', source, true))
                this.__declarations.set(name, {value, priority:''});
        }
        get cssText() { return super.cssText; }
        set cssText(value) {
            this.__declarations.clear();
            for (const [name, text] of host('cssDeclarationList', String(value), true))
                this.__declarations.set(name, {value:text, priority:''});
            this.__rule.__changed();
        }
        setProperty(name, value, priority = '') {
            // Important declarations are not allowed in keyframe blocks.
            if (String(priority) !== '') return;
            name = declarationName(name);
            if ((name === 'animation' || name.startsWith('animation-')) && name !== 'animation-timing-function') return;
            super.setProperty(name, value, '');
        }
    }
    class CSSKeyframeRule extends CSSRule {
        constructor(sheet, text, token) {
            if (token !== cssKeyframesRuleToken) throw new TypeError('Illegal constructor');
            super(sheet, text, cssRuleConstructionToken);
            const parsed = host('cssKeyframeBlock', text);
            if (parsed === null) throw new DOMException('Invalid keyframe block', 'SyntaxError');
            this.__offsets = parsed[0];
            const open = cssRuleBlockStart(text);
            this.__style = ruleStyleProxy(new KeyframeStyleDeclaration(this, text.slice(open + 1, text.lastIndexOf('}'))));
        }
        get type() { return 8; }
        get keyText() { return cssKeyframeText(this.__offsets); }
        set keyText(value) {
            const offsets = host('cssKeyframeOffsets', String(value));
            if (offsets === null) throw new DOMException('Invalid keyframe selector', 'SyntaxError');
            this.__offsets = offsets;
            this.__changed();
        }
        get style() { return this.__style; }
        get cssText() { return this.keyText + ' { ' + this.__style.cssText + ' }'; }
        set cssText(_value) {}
        __changed() { this.parentRule?.__changed(); }
    }
    class CSSKeyframesRule extends CSSRule {
        constructor(sheet, text, token) {
            if (token !== cssKeyframesRuleToken) throw new TypeError('Illegal constructor');
            super(sheet, text, cssRuleConstructionToken);
            const open = cssRuleBlockStart(text);
            const prelude = text.slice(0, open).replace(/^@(?:-webkit-)?keyframes\s*/i, '');
            const name = host('cssKeyframeName', prelude);
            if (name === null) throw new DOMException('Invalid keyframes name', 'SyntaxError');
            this.__name = name;
            this.__rules = [];
            this.__ruleList = readonlyRuleList(this.__rules);
            for (const source of scanCssRules(text.slice(open + 1, text.lastIndexOf('}'))))
                this.__append(source);
            const proxy = new Proxy(this, {
                get(target, property, receiver) {
                    if (typeof property === 'string' && /^(0|[1-9][0-9]*)$/.test(property))
                        return target.__rules[Number(property)];
                    return Reflect.get(target, property, receiver);
                },
                set(target, property, value, receiver) {
                    if (typeof property === 'string' && /^(0|[1-9][0-9]*)$/.test(property)) return false;
                    return Reflect.set(target, property, value, receiver);
                },
                has(target, property) {
                    if (typeof property === 'string' && /^(0|[1-9][0-9]*)$/.test(property))
                        return Number(property) < target.__rules.length;
                    return Reflect.has(target, property);
                },
                getOwnPropertyDescriptor(target, property) {
                    if (typeof property === 'string' && /^(0|[1-9][0-9]*)$/.test(property))
                        return Number(property) < target.__rules.length ?
                            {value:target.__rules[Number(property)], writable:false, enumerable:true, configurable:true} : undefined;
                    return Reflect.getOwnPropertyDescriptor(target, property);
                },
                ownKeys(target) {
                    return target.__rules.map((_, index) => String(index)).concat(Reflect.ownKeys(target));
                },
                defineProperty(target, property, descriptor) {
                    if (typeof property === 'string' && /^(0|[1-9][0-9]*)$/.test(property)) return false;
                    return Reflect.defineProperty(target, property, descriptor);
                },
                deleteProperty(target, property) {
                    if (typeof property === 'string' && /^(0|[1-9][0-9]*)$/.test(property)) return false;
                    return Reflect.deleteProperty(target, property);
                },
                preventExtensions() { return false; }
            });
            for (const rule of this.__rules) rule.__parentRule = proxy;
            return proxy;
        }
        get type() { return 7; }
        get name() { return this.__name; }
        set name(value) {
            // The CSSOM setter takes a DOMString name, not a parsed CSS identifier.
            this.__name = String(value);
            this.__changed();
        }
        get cssRules() { return this.__ruleList; }
        get cssText() {
            return '@keyframes ' + host('cssKeyframeNameText', this.__name) + ' {\n' +
                this.__rules.map(rule => '  ' + rule.cssText).join('\n') + '\n}';
        }
        set cssText(_value) {}
        __append(source) {
            if (this.__rules.length >= 256 || host('cssKeyframeBlock', source) === null) return false;
            const rule = new CSSKeyframeRule(this.parentStyleSheet, source, cssKeyframesRuleToken);
            rule.__parentRule = this;
            this.__rules.push(rule);
            return true;
        }
        appendRule(source) {
            if (this.__append(String(source))) this.__changed();
        }
        __find(key) {
            const offsets = host('cssKeyframeOffsets', String(key));
            if (offsets === null) return -1;
            const sorted = offsets.slice().sort((a,b) => a-b).join(',');
            for (let index = this.__rules.length - 1; index >= 0; index--)
                if (this.__rules[index].__offsets.slice().sort((a,b) => a-b).join(',') === sorted)
                    return index;
            return -1;
        }
        findRule(key) { return this.__rules[this.__find(key)] || null; }
        deleteRule(key) {
            const index = this.__find(key);
            if (index < 0) return;
            const [rule] = this.__rules.splice(index, 1);
            rule.__parentRule = rule.__parentStyleSheet = null;
            this.__changed();
        }
        __changed() {
            if (this.parentRule) this.parentRule.__changed();
            else this.parentStyleSheet?.__rulesChanged();
        }
    }
    for (const [name, value] of [['KEYFRAMES_RULE', 7], ['KEYFRAME_RULE', 8]])
        for (const object of [CSSRule, CSSRule.prototype])
            Object.defineProperty(object, name, {value, enumerable:true});
    for (const constructor of [CSSKeyframesRule, CSSKeyframeRule])
        Object.defineProperty(constructor.prototype, Symbol.toStringTag,
            {value:constructor.name, configurable:true});
    Object.assign(windowObject, {CSSKeyframesRule, CSSKeyframeRule});
