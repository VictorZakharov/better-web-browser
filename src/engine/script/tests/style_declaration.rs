use super::*;

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
