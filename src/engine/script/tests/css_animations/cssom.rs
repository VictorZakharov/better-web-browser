use super::assert_script;

#[test]
fn keyframe_declarations_ignore_important_in_source_and_cssom_edits() {
    assert_script(
        "@keyframes fade{from{opacity:.2;opacity:.9!important}to{opacity:1}}",
        r#"
        const style = document.styleSheets[0].cssRules[0][0].style;
        check('important source ignored', style.opacity === '.2' && style.getPropertyPriority('opacity') === '');
        style.setProperty('opacity','.8','important');
        check('important setter ignored', style.opacity === '.2');
        style.cssText = 'opacity:.4;opacity:.9!important;color:red!important';
        check('cssText filters priorities', style.opacity === '.4' && style.color === '');
    "#,
    );
}

#[test]
fn indexed_keyframe_getters_track_live_append_and_delete() {
    assert_script(
        "@keyframes fade{from{opacity:0}to{opacity:1}}",
        r#"
        const rule = document.styleSheets[0].cssRules[0];
        const first = rule[0];
        rule.deleteRule('from');
        check('index shifted', rule[0].keyText === '100%' && !(1 in rule));
        rule.appendRule('50%{opacity:.5}');
        check('new indexed getter', rule[1] === rule.findRule('50%') && 1 in rule);
        check('old detached', first.parentRule === null);
        let rejected = false;
        try { 'use strict'; Object.defineProperty(rule,'0',{value:first}); } catch(e) { rejected = true; }
        // Named keyframe lookup must still read the live list, never an authored numeric field.
        check('lookup remains live', rule.findRule('to') === rule.cssRules[0]);
    "#,
    );
}

#[test]
fn cssom_keyframe_rule_hierarchy_and_live_rule_list() {
    assert_script(
        "@keyframes fade{from{opacity:0}to{opacity:1}}",
        r#"
        const sheet = document.styleSheets[0], rule = sheet.cssRules[0];
        const list = rule.cssRules;
        check('keyframes interface', rule instanceof CSSKeyframesRule && rule instanceof CSSRule);
        check('not grouping', !(rule instanceof CSSGroupingRule));
        check('rule types', rule.type === CSSRule.KEYFRAMES_RULE && rule.cssRules[0].type === CSSRule.KEYFRAME_RULE);
        check('name', rule.name === 'fade');
        check('parent', rule.cssRules[0].parentRule === rule && rule.cssRules[0].parentStyleSheet === sheet);
        check('from normalized', rule.cssRules[0].keyText === '0%');
        check('indexed getter', rule[0] === list[0] && 0 in rule && !(9 in rule));
        rule.appendRule('50% {opacity:.3}');
        check('live list', rule.cssRules === list && list.length === 3);
        check('find alias', rule.findRule('from') === list[0]);
        rule.deleteRule('from');
        check('deleted', list.length === 2 && rule.findRule('0%') === null);
        "#,
    );
}

#[test]
fn find_and_delete_choose_last_equivalent_selector_list() {
    assert_script(
        "@keyframes fade{from,50%{opacity:0}50%,0%{opacity:.2}to{opacity:1}}",
        r#"
        const rule = document.styleSheets[0].cssRules[0];
        check('last matching', rule.findRule('from, 50%') === rule.cssRules[1]);
        const detached = rule.cssRules[1];
        rule.deleteRule('50%, 0%');
        check('detached parent', detached.parentRule === null && detached.parentStyleSheet === null);
        check('earlier remains', rule.findRule('0%,50%') === rule.cssRules[0]);
        rule.deleteRule('99%');
        check('no-op delete', rule.cssRules.length === 2);
        "#,
    );
}

#[test]
fn cssom_edits_update_running_effect_without_resetting_playback_time() {
    assert_script(
        "@keyframes fade{from{opacity:0}to{opacity:1}}
        #target{animation:fade 1s linear both paused}",
        r#"
        const animation = target.getAnimations()[0];
        animation.currentTime = 500;
        const rule = document.styleSheets[0].cssRules[0];
        rule.findRule('to').style.opacity = '.4';
        check('same object', target.getAnimations()[0] === animation);
        check('same time', animation.currentTime === 500);
        check('updated sample', getComputedStyle(target).opacity === '0.2');
        rule.appendRule('50% {opacity:.9}');
        check('new intermediate', getComputedStyle(target).opacity === '0.9');
        rule.deleteRule('50%');
        check('restored path', getComputedStyle(target).opacity === '0.2');
        "#,
    );
}

#[test]
fn invalid_key_text_throws_without_mutating_rule() {
    assert_script(
        "@keyframes fade{from{opacity:0}to{opacity:1}}",
        r#"
        const rule = document.styleSheets[0].cssRules[0];
        const first = rule.cssRules[0];
        let thrown = false;
        try { first.keyText = '120%'; } catch(error) { thrown = error.name === 'SyntaxError'; }
        check('SyntaxError', thrown);
        check('unchanged', first.keyText === '0%');
        first.keyText = '25%, 75%';
        check('valid edit', first.keyText === '25%, 75%');
        rule.appendRule('bad {opacity:.3}');
        check('invalid append ignored', rule.cssRules.length === 2);
        "#,
    );
}

#[test]
fn constructed_sheet_keyframes_are_executable_after_adoption() {
    assert_script(
        "",
        r#"
        const sheet = new CSSStyleSheet();
        sheet.replaceSync('@keyframes fade{from{opacity:0}to{opacity:1}} #target{animation:fade 1s linear both paused}');
        document.adoptedStyleSheets = [sheet];
        const animation = target.getAnimations()[0];
        check('adopted animation', animation instanceof CSSAnimation);
        animation.currentTime = 500;
        check('adopted sample', getComputedStyle(target).opacity === '0.5');
        document.adoptedStyleSheets = [];
        check('removed', target.getAnimations().length === 0);
        "#,
    );
}
