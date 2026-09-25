use super::*;

#[test]
fn inset_clip_path_is_exposed_without_claiming_unsupported_shapes() {
    let (dom, outcome) = execute_html(
        r#"<style>main{clip-path:inset(10% 2px)}</style><body><main></main><output></output><script>
        const main = document.querySelector('main');
        const style = getComputedStyle(main);
        document.querySelector('output').textContent = [
            CSS.supports('clip-path', 'inset(10%)'),
            !CSS.supports('clip-path', 'circle(50%)'),
            style.clipPath === 'inset(10% 2px 10% 2px)',
            main.style.clipPath === ''
        ].every(Boolean) ? 'yes' : style.clipPath;
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "yes"
    );
}

#[test]
fn style_named_properties_do_not_claim_unknown_capabilities() {
    let (dom, outcome) = execute_html(
        r#"<style>p{color:red}</style><body><output>no</output><script>
        const checks = [];
        const element = document.createElement('div');
        for (const style of [element.style, document.styleSheets[0].cssRules[0].style]) {
            for (const name of ['breezeUnknownProperty', 'perspective', 'WebkitPerspective',
                                'webkitPerspective', 'transition', 'filter']) {
                checks.push(style[name] === undefined, !(name in style));
            }
            for (const name of ['display', 'width', 'backgroundColor', 'background-color', 'cssFloat'])
                checks.push(typeof style[name] === 'string', name in style);
            style.backgroundColor = 'blue';
            checks.push(style['background-color'] === 'blue',
                style.getPropertyValue('background-color') === 'blue');
            style.cssFloat = 'right';
            checks.push(style.getPropertyValue('float') === 'right');
            const before = style.cssText;
            style.breezeUnknownProperty = 42;
            const marker = Symbol('marker');
            style[marker] = 'expando';
            checks.push(style.breezeUnknownProperty === 42, style[marker] === 'expando',
                style.cssText === before, style[Symbol('unknown')] === undefined);
            delete style.breezeUnknownProperty;
            checks.push(style.breezeUnknownProperty === undefined);
            checks.push(style.getPropertyValue('absent-property') === '');
        }
        checks.push(CSS.supports('transform', 'translateX(10px)'),
            !CSS.supports('transform', 'translate3d(10px, 0, 0)'),
            !CSS.supports('perspective', 'initial'), !CSS.supports('transform-style', 'preserve-3d'));
        document.querySelector('output').textContent = checks.every(Boolean) ? 'yes' : checks.join();
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "yes"
    );
}

#[test]
fn react_webkit_clamp_properties_round_trip_through_named_style_members() {
    let (dom, outcome) = execute_html(
        r#"<body><span id="target"></span><output></output><script>
        const target = document.getElementById('target');
        Object.assign(target.style, {
            WebkitLineClamp: 2,
            WebkitBoxOrient: 'vertical',
            display: '-webkit-box'
        });
        document.querySelector('output').textContent = [
            target.style.WebkitLineClamp,
            target.style.getPropertyValue('-webkit-line-clamp'),
            target.style.WebkitBoxOrient,
            target.style.getPropertyValue('-webkit-box-orient'),
            target.style.display,
            target.getAttribute('style')
        ].join('|');
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "2|2|vertical|vertical|-webkit-box|-webkit-line-clamp: 2; \
         -webkit-box-orient: vertical; display: -webkit-box"
    );
}

#[test]
fn inline_style_writes_use_internal_attribute_steps_not_overridden_set_attribute() {
    let (dom, outcome) = execute_html(
        r#"<body><output></output><script>
        const element = document.createElement('div');
        let authorSetCalls = 0;
        let authorGetCalls = 0;
        const originalGetAttribute = element.getAttribute;
        element.setAttribute = () => { authorSetCalls++; throw Error('author setAttribute called'); };
        element.getAttribute = () => { authorGetCalls++; throw Error('author getAttribute called'); };
        const records = [];
        new MutationObserver(items => records.push(...items.map(item =>
            item.attributeName + ':' + item.oldValue)))
            .observe(element, {attributes:true, attributeOldValue:true});
        element.style.setProperty('color', 'red');
        element.style.backgroundColor = 'blue';
        element.style.removeProperty('color');
        element.style.cssText = 'width: 12px';
        queueMicrotask(() => {
            document.querySelector('output').textContent = [
                authorSetCalls,
                authorGetCalls,
                originalGetAttribute.call(element, 'style'),
                element.style.width,
                records.join('|')
            ].join(';');
        });
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "0;0;width: 12px;12px;style:null|style:color: red|style:color: red; background-color: blue|style:background-color: blue"
    );
}

#[test]
fn inline_style_writes_still_trigger_custom_element_attribute_reactions() {
    let (dom, outcome) = execute_html(
        r#"<body><output></output><script>
        const changes = [];
        class StyledElement extends HTMLElement {
            static get observedAttributes() { return ['style']; }
            attributeChangedCallback(name, oldValue, newValue, namespace) {
                changes.push([name, oldValue, newValue, namespace].join(':'));
            }
        }
        customElements.define('x-styled', StyledElement);
        const element = document.createElement('x-styled');
        element.style.color = 'red';
        element.style.setProperty('width', '12px');
        element.style.removeProperty('color');
        document.querySelector('output').textContent = changes.join('|');
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "style::color: red:|style:color: red:color: red; width: 12px:|style:color: red; width: 12px:width: 12px:"
    );
}
