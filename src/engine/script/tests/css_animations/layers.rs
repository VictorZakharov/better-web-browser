use super::assert_script;

#[test]
fn higher_precedence_layer_wins_over_later_source_order() {
    assert_script(
        "@layer base, overrides; @layer overrides{@keyframes fade{from{opacity:0}to{opacity:.4}}} @layer base{@keyframes fade{from{opacity:0}to{opacity:1}}} #target{animation:fade 1s linear both paused}",
        r#"
        target.getAnimations()[0].currentTime = 500;
        check('layer precedence', getComputedStyle(target).opacity === '0.2');
    "#,
    );
}

#[test]
fn unlayered_definition_wins_over_any_explicit_layer() {
    assert_script(
        "@keyframes fade{from{opacity:0}to{opacity:.6}} @layer later{@keyframes fade{from{opacity:0}to{opacity:1}}} #target{animation:fade 1s linear both paused}",
        r#"
        target.getAnimations()[0].currentTime = 500;
        check('implicit final layer', getComputedStyle(target).opacity === '0.3');
    "#,
    );
}

#[test]
fn parent_layer_has_precedence_over_its_sublayers() {
    assert_script(
        "@layer theme{@keyframes fade{from{opacity:0}to{opacity:.8}} @layer nested{@keyframes fade{from{opacity:0}to{opacity:1}}}} #target{animation:fade 1s linear both paused}",
        r#"
        target.getAnimations()[0].currentTime = 500;
        check('parent layer final', getComputedStyle(target).opacity === '0.4');
    "#,
    );
}

#[test]
fn reopened_layer_uses_last_definition_across_stylesheets() {
    assert_script(
        "@layer theme{@keyframes fade{from{opacity:0}to{opacity:1}}} #target{animation:fade 1s linear both paused}",
        r#"
        const sheet = new CSSStyleSheet();
        sheet.replaceSync('@layer theme{@keyframes fade{from{opacity:0}to{opacity:.4}}}');
        document.adoptedStyleSheets = [sheet];
        target.getAnimations()[0].currentTime = 500;
        check('named layer reopened', getComputedStyle(target).opacity === '0.2');
    "#,
    );
}

#[test]
fn anonymous_layers_remain_distinct_and_follow_occurrence_order() {
    assert_script(
        "@layer{@keyframes fade{from{opacity:0}to{opacity:1}}} @layer{@keyframes fade{from{opacity:0}to{opacity:.4}}} #target{animation:fade 1s linear both paused}",
        r#"
        target.getAnimations()[0].currentTime = 500;
        check('anonymous precedence', getComputedStyle(target).opacity === '0.2');
    "#,
    );
}

#[test]
fn disabled_conditional_layer_does_not_change_keyframe_order() {
    assert_script(
        "@media (min-width:99999px){@layer high,low;} @layer low,high; @layer high{@keyframes fade{from{opacity:0}to{opacity:.4}}} @layer low{@keyframes fade{from{opacity:0}to{opacity:1}}} #target{animation:fade 1s linear both paused}",
        r#"
        target.getAnimations()[0].currentTime = 500;
        check('disabled ordering absent', getComputedStyle(target).opacity === '0.2');
    "#,
    );
}

#[test]
fn scoped_group_keyframes_are_available_in_their_tree_scope() {
    assert_script(
        "@scope (main){@keyframes fade{from{opacity:0}to{opacity:1}} #target{animation:fade 1s linear both paused}}",
        r#"
        target.getAnimations()[0].currentTime = 500;
        check('scope group parsed', getComputedStyle(target).opacity === '0.5');
    "#,
    );
}
