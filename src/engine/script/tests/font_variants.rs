use super::*;

#[test]
fn font_variants_use_actual_inherited_css_values_and_supports_agrees_with_the_parser() {
    let (dom, outcome) = execute_html(
        r#"<style>
        #parent{font-variant-ligatures:no-contextual common-ligatures;
            font-variant-numeric:slashed-zero tabular-nums oldstyle-nums}
        #child{font-variant-ligatures:none contextual;font-variant-numeric:lining-nums oldstyle-nums}
    </style><body><div id=parent><span id=child>x</span></div><output></output><script>
        const assert=(v,m)=>{if(!v)throw Error(m)};
        const child=document.querySelector('#child');
        const value=name=>getComputedStyle(child).getPropertyValue(name);
        assert(value('font-variant-ligatures')==='common-ligatures no-contextual','inherited ligatures');
        assert(value('font-variant-numeric')==='oldstyle-nums tabular-nums slashed-zero','canonical numeric ordering');
        assert(CSS.supports('font-variant-ligatures','no-common-ligatures contextual'),'supported ligatures');
        assert(!CSS.supports('font-variant-ligatures','contextual no-contextual'),'conflicting ligatures');
        assert(CSS.supports('font-variant-numeric','stacked-fractions ordinal'),'supported numeric');
        assert(!CSS.supports('font-variant-numeric','diagonal-fractions stacked-fractions'),'conflicting fractions');
        assert(!CSS.supports('font-variant','small-caps'),'unimplemented synthesis is not advertised');
        child.style.setProperty('font-variant','ordinal no-common-ligatures');
        assert(value('font-variant-ligatures')==='no-common-ligatures','shorthand ligatures');
        assert(value('font-variant-numeric')==='ordinal','shorthand resets inherited numeric groups');
        assert(value('font-variant')==='no-common-ligatures ordinal','computed shorthand');
        child.style.setProperty('font-variant','small-caps');
        assert(value('font-variant')==='no-common-ligatures ordinal','invalid shorthand is atomic');
        child.style.setProperty('font-variant-ligatures','unset');
        child.style.setProperty('font-variant-numeric','initial');
        assert(value('font-variant-ligatures')==='common-ligatures no-contextual','unset inherits');
        assert(value('font-variant-numeric')==='normal','initial resets');
        child.style.setProperty('font-variant','inherit');
        assert(value('font-variant-numeric')==='oldstyle-nums tabular-nums slashed-zero','inherit shorthand');
        child.style.setProperty('font','20px Arial');
        assert(value('font-variant-ligatures')==='normal'&&value('font-variant-numeric')==='normal','font resets variants');
        document.querySelector('output').textContent='passed';
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "passed"
    );
}
