use super::assert_script;

#[test]
fn variable_keyframes_recompute_when_custom_property_changes() {
    assert_script(
        "@keyframes fade{from{opacity:0}to{opacity:var(--end)}}
        #target{--end:.8;animation:fade 1s linear both paused}",
        r#"
        const animation = target.getAnimations()[0];
        animation.currentTime = 500;
        check('initial variable', getComputedStyle(target).opacity === '0.4');
        target.style.setProperty('--end','.4');
        check('identity retained', target.getAnimations()[0] === animation);
        check('variable updated', getComputedStyle(target).opacity === '0.2');
        "#,
    );
}

#[test]
fn font_relative_keyframes_use_computed_target_font_metrics() {
    assert_script(
        "@keyframes size{from{width:1em}to{width:3em}}
        #target{font-size:20px;animation:size 1s linear both paused}",
        r#"
        const animation = target.getAnimations()[0];
        animation.currentTime = 500;
        check('computed units', getComputedStyle(target).width === '40px');
        target.style.fontSize = '10px';
        check('updated units', getComputedStyle(target).width === '20px');
        "#,
    );
}

#[test]
fn implicit_endpoints_follow_new_underlying_style_not_own_old_overlay() {
    assert_script(
        "@keyframes partial{50%{opacity:1}}
        #target{opacity:.2;animation:partial 1s linear both paused}",
        r#"
        const animation = target.getAnimations()[0];
        animation.currentTime = 250;
        check('initial neutral endpoint', getComputedStyle(target).opacity === '0.6');
        target.style.opacity = '.6';
        check('changed neutral endpoint', getComputedStyle(target).opacity === '0.8');
        check('same clock', animation.currentTime === 250);
        "#,
    );
}

#[test]
fn box_shorthand_keyframes_drive_all_four_native_edges() {
    assert_script(
        "@keyframes move{from{margin:0px}to{margin:20px 40px 60px 80px}}
        #target{animation:move 1s linear both paused}",
        r#"
        const animation = target.getAnimations()[0];
        animation.currentTime = 500;
        const style = getComputedStyle(target);
        check('top', style.marginTop === '10px');
        check('right', style.marginRight === '20px');
        check('bottom', style.marginBottom === '30px');
        check('left', style.marginLeft === '40px');
        "#,
    );
}

#[test]
fn inherited_color_keyframe_resolves_parent_before_sampling() {
    assert_script(
        "main{color:blue}@keyframes colorize{from{color:red}to{color:inherit}}
        #target{animation:colorize 1s linear both paused}",
        r#"
        const animation = target.getAnimations()[0];
        animation.currentTime = 500;
        check('inherited endpoint', getComputedStyle(target).color === 'rgb(128, 0, 128)');
        "#,
    );
}

#[test]
fn invalid_declaration_does_not_overwrite_valid_sibling_frame_value() {
    assert_script(
        "@keyframes fade{from{opacity:0;not-a-property:123}to{opacity:1;color:garbage}}
        #target{animation:fade 1s linear both paused}",
        r#"
        const animation = target.getAnimations()[0];
        animation.currentTime = 500;
        check('valid sibling', getComputedStyle(target).opacity === '0.5');
        check('invalid color absent', !('color' in animation.effect.getKeyframes()[1]));
        "#,
    );
}
