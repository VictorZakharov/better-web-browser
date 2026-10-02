use super::assert_script;

#[test]
fn replacing_an_adopted_sheet_preserves_matching_animation_identity() {
    assert_script(
        "",
        r#"
        const sheet = new CSSStyleSheet();
        sheet.replaceSync('@keyframes fade{from{opacity:0}to{opacity:1}} #target{animation:fade 1s linear both paused}');
        document.adoptedStyleSheets = [sheet];
        const animation = target.getAnimations()[0];
        animation.currentTime = 500;
        sheet.replaceSync('@keyframes fade{from{opacity:0}to{opacity:.4}} #target{animation:fade 1s linear both paused}');
        check('same named animation', target.getAnimations()[0] === animation);
        check('clock retained', animation.currentTime === 500);
        check('replaced source sampled', getComputedStyle(target).opacity === '0.2');
        "#,
    );
}

#[test]
fn removing_and_restoring_style_source_restarts_the_animation() {
    assert_script(
        "@keyframes fade{from{opacity:0}to{opacity:1}} #target{animation:fade 1s linear both paused}",
        r#"
        const style = document.querySelector('style');
        const animation = target.getAnimations()[0];
        animation.currentTime = 500;
        style.remove();
        check('removed source cancels', target.getAnimations().length === 0 && animation.playState === 'idle');
        check('overlay gone', getComputedStyle(target).opacity === '1');
        document.head.append(style);
        const next = target.getAnimations()[0];
        check('restored source restarts', next !== animation && next.currentTime === 0);
        "#,
    );
}

#[test]
fn keyframe_name_edits_change_lookup_without_mutating_animation_name_declaration() {
    assert_script(
        "@keyframes fade{from{opacity:0}to{opacity:1}} #target{animation:fade 1s linear both paused}",
        r#"
        const rule = document.styleSheets[0].cssRules[0];
        const animation = target.getAnimations()[0];
        rule.name = 'renamed';
        check('old lookup no longer exists', target.getAnimations().length === 0);
        check('old effect canceled', animation.playState === 'idle');
        check('author declaration unchanged', getComputedStyle(target).animationName === 'fade');
        target.style.animationName = 'renamed';
        const next = target.getAnimations()[0];
        next.currentTime = 500;
        check('new name resolves', next.animationName === 'renamed' && getComputedStyle(target).opacity === '0.5');
        "#,
    );
}

#[test]
fn ordinary_rule_cssom_edit_starts_and_removes_animation() {
    assert_script(
        "@keyframes fade{from{opacity:0}to{opacity:1}} #target{opacity:.8}",
        r#"
        const style = document.styleSheets[0].cssRules[1].style;
        check('style owns rule', style.parentRule === document.styleSheets[0].cssRules[1]);
        style.setProperty('animation','fade 1s linear both paused');
        const animation = target.getAnimations()[0];
        animation.currentTime = 500;
        check('rule setter starts', getComputedStyle(target).opacity === '0.5');
        style.removeProperty('animation');
        check('rule removal cancels', target.getAnimations().length === 0 && animation.playState === 'idle');
        check('underlying restored', getComputedStyle(target).opacity === '0.8');
        "#,
    );
}

#[test]
fn duplicate_definitions_are_reselected_when_last_rule_is_deleted() {
    assert_script(
        "@keyframes fade{from{opacity:0}to{opacity:.4}} @keyframes fade{from{opacity:0}to{opacity:1}} #target{animation:fade 1s linear both paused}",
        r#"
        const animation = target.getAnimations()[0];
        animation.currentTime = 500;
        check('last definition', getComputedStyle(target).opacity === '0.5');
        document.styleSheets[0].deleteRule(1);
        check('identity survives definition change', target.getAnimations()[0] === animation);
        check('earlier definition selected', getComputedStyle(target).opacity === '0.2');
        "#,
    );
}

#[test]
fn custom_property_edits_in_rules_update_frames_not_authored_inline_styles() {
    assert_script(
        "@keyframes fade{from{opacity:0}to{opacity:var(--end)}} #target{--end:.8;animation:fade 1s linear both paused}",
        r#"
        const animation = target.getAnimations()[0];
        animation.currentTime = 500;
        document.styleSheets[0].cssRules[1].style.setProperty('--end','.4');
        check('variable edit samples', getComputedStyle(target).opacity === '0.2');
        check('same object and clock', target.getAnimations()[0] === animation && animation.currentTime === 500);
        check('no inline materialization', target.getAttribute('style') === null);
        "#,
    );
}
