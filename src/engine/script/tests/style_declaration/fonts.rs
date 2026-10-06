use super::*;

fn assert_fonts(script: &str) {
    let source = format!(
        r#"<style>#target{{font-size:30px}}</style><body><div id=target></div>
        <output></output><script>
        const target=document.querySelector('#target');
        const assert=(value,message)=>{{if(!value)throw Error(message)}};
        {script}
        document.querySelector('output').textContent='passed';</script>"#
    );
    let (dom, outcome) = execute_html(&source);
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "passed"
    );
}

#[test]
fn inline_and_rule_font_shorthands_replace_previous_longhands_and_preserve_relative_units() {
    assert_fonts(
        r#"
        const sheet=new CSSStyleSheet();
        sheet.replaceSync('#target{}');
        document.adoptedStyleSheets=[sheet];
        for (const style of [target.style,sheet.cssRules[0].style]) {
            style.font='16px Arial';
            style.fontSize='22px';
            assert(style.font==='22px Arial','serialize edited size');
            style.font='italic 600 2em / 150% Arial';
            assert(style.fontSize==='2em','specified relative size');
            assert(style.lineHeight==='150%','specified relative line height');
            assert(style.fontStyle==='italic'&&style.fontWeight==='600','style and weight');
            assert(style.font==='italic 600 2em / 150% Arial','shorthand serialization');
            assert(style.getPropertyValue('font-feature-settings')==='normal','feature reset');
            assert(style.getPropertyValue('font-kerning')==='auto','kerning reset');
            assert(style.getPropertyValue('font-variant-ligatures')==='normal','ligature reset');
            style.font='20px Arial';
            assert(style.fontStyle==='normal'&&style.fontWeight==='400','omitted values reset');
            assert(style.lineHeight==='normal','omitted line height resets');
            assert(style.fontSize==='20px','old longhand no longer overrides shorthand');
            assert(getComputedStyle(target).fontSize==='20px','native cascade agrees');
            style.removeProperty('font');
        }
    "#,
    );
}

#[test]
fn font_reset_only_values_and_mixed_priorities_prevent_lossy_shorthand_serialization() {
    assert_fonts(
        r#"
        const style=target.style;
        style.font='20px Arial';
        style.setProperty('font-feature-settings','"liga" off');
        assert(style.font==='','noninitial feature cannot serialize via font');
        style.font='20px Arial';
        style.setProperty('font-size','24px','important');
        assert(style.font==='','mixed priorities cannot serialize');
        style.setProperty('font','bold 30px Arial','important');
        assert(style.getPropertyPriority('font')==='important','uniform shorthand priority');
        assert(style.getPropertyPriority('font-size')==='important','priority propagated');
        assert(style.font==='700 30px Arial','canonical numeric weight');
        assert(getComputedStyle(target).fontSize==='30px','important reset wins');
        assert(style.removeProperty('font')==='700 30px Arial','remove returns old shorthand');
        for (const name of ['font-size','font-weight','font-style','font-family','line-height',
            'font-feature-settings','font-kerning','font-variant-ligatures','font-variant-numeric'])
            assert(style.getPropertyValue(name)==='','remove clears '+name);
    "#,
    );
}

#[test]
fn declaration_lists_expand_shorthands_in_order_and_keep_important_longhand_winners() {
    assert_fonts(
        r#"
        const style=target.style;
        style.cssText='font-size:40px!important;font:20px Arial;font-weight:600';
        assert(style.fontSize==='40px','important size survives later shorthand');
        assert(style.fontWeight==='600','later ordinary longhand wins');
        assert(style.font==='','mixed priority shorthand is not representable');
        assert(getComputedStyle(target).fontSize==='40px','cascade sees important winner');
        style.cssText='font:20px Arial!important;font-size:40px;font-weight:600';
        assert(style.fontSize==='20px'&&style.fontWeight==='400','important shorthand longhands win');
        assert(style.font==='20px Arial','uniform shorthand reconstructs');
        style.cssText='font-variant:oldstyle-nums no-common-ligatures;font-variant-numeric:ordinal';
        assert(style.getPropertyValue('font-variant-ligatures')==='no-common-ligatures','ligature retained');
        assert(style.getPropertyValue('font-variant')==='no-common-ligatures ordinal','edited variant serializes');
        style.setProperty('font-variant','inherit','important');
        assert(style.getPropertyValue('font-variant')==='inherit','wide variant serialization');
        assert(style.getPropertyPriority('font-variant')==='important','variant priority');
        assert(style.removeProperty('font-variant')==='inherit','variant removal returns shorthand');
        assert(style.getPropertyValue('font-variant-numeric')==='','variant longhand removed');
    "#,
    );
}

#[test]
fn font_css_wide_keywords_invalid_atomic_updates_and_supports_follow_native_grammar() {
    assert_fonts(
        r#"
        const style=target.style;
        for (const value of ['italic 600 20px Arial','20px/1.5 Arial','2em "Family, One", serif'])
            assert(CSS.supports('font',value),'supported '+value);
        for (const value of ['bold bold 20px Arial','italic italic 20px Arial',
            'small-caps 20px Arial','condensed 20px Arial','20px / -1 Arial','20px',
            'caption','1001 20px Arial']) {
            assert(!CSS.supports('font',value),'unsupported '+value);
            style.font='20px Arial';
            style.font=value;
            assert(style.font==='20px Arial','invalid update atomic '+value);
        }
        for (const value of ['inherit','initial','unset','revert','revert-layer']) {
            style.font=value;
            assert(style.font===value,'wide font serialization '+value);
            assert(style.fontSize===value,'wide size '+value);
            style.fontSize='20px';
            assert(style.font==='','mixed wide and ordinary cannot serialize');
        }
    "#,
    );
}

#[test]
fn equal_shorthand_assignments_do_not_create_spurious_attribute_mutations() {
    assert_fonts(
        r#"
        const style=target.style;
        style.font='italic 600 20px Arial';
        const observer=new MutationObserver(()=>{});
        observer.observe(target,{attributes:true,attributeFilter:['style']});
        style.font='italic 600 20px Arial';
        style.setProperty('font','italic 600 20px Arial');
        assert(observer.takeRecords().length===0,'equal font writes are no-ops');
        style.fontSize='22px';
        assert(observer.takeRecords().length===1,'actual longhand edit emits mutation');
        style.font='italic 600 20px Arial';
        assert(observer.takeRecords().length===1,'changed shorthand emits one mutation');
        assert(style.fontSize==='20px','changed shorthand replaces longhand');
        style.setProperty('font-variant','ordinal no-common-ligatures');
        observer.takeRecords();
        style.setProperty('font-variant','no-common-ligatures ordinal');
        assert(observer.takeRecords().length===0,'equivalent variant writes are no-ops');
    "#,
    );
}
