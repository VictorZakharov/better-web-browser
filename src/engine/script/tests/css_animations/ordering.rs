use super::assert_script;

#[test]
fn css_animation_query_order_is_animation_name_order_after_reordering() {
    assert_script(
        "@keyframes a{from{opacity:0}to{opacity:1}} @keyframes b{from{opacity:.2}to{opacity:.8}} #target{animation:a 1s linear both paused,b 1s linear both paused}",
        r#"
        const first = target.getAnimations();
        first[0].currentTime = 250; first[1].currentTime = 500;
        target.style.animationName = 'b,a';
        const next = target.getAnimations();
        check('query follows list', next[0] === first[1] && next[1] === first[0]);
        check('times preserved', next[0].currentTime === 500 && next[1].currentTime === 250);
        check('last CSS effect wins', getComputedStyle(target).opacity === '0.25');
        check('document uses same order', document.getAnimations()[0] === next[0]);
    "#,
    );
}

#[test]
fn script_effects_have_higher_composite_order_than_css_animations() {
    assert_script(
        "@keyframes fade{from{opacity:0}to{opacity:1}} #target{animation:fade 1s linear both paused}",
        r#"
        const css = target.getAnimations()[0]; css.currentTime = 500;
        const script = target.animate([{opacity:.8},{opacity:1}],{duration:1000,fill:'both'});
        script.pause(); script.currentTime = 500;
        const list = target.getAnimations();
        check('CSS before script', list[0] === css && list[1] === script);
        check('script wins', getComputedStyle(target).opacity === '0.9');
        script.cancel();
        check('CSS restored', getComputedStyle(target).opacity === '0.5');
        check('CSS brand', Object.prototype.toString.call(css) === '[object CSSAnimation]');
    "#,
    );
}

#[test]
fn document_css_animation_order_uses_tree_order_not_reverse_discovery() {
    assert_script(
        "@keyframes fade{from{opacity:0}to{opacity:1}} .animated{animation:fade 1s both paused}",
        r#"
        const sibling = document.createElement('div');
        document.getElementById('parent').append(sibling);
        sibling.className = 'animated';
        const second = sibling.getAnimations()[0];
        target.className = 'animated';
        const first = target.getAnimations()[0];
        check('tree order', document.getAnimations()[0] === first && document.getAnimations()[1] === second);
    "#,
    );
}

#[test]
fn duplicate_names_match_existing_animations_last_to_first() {
    assert_script(
        "@keyframes fade{from{opacity:0}to{opacity:1}} #target{animation:fade 1s linear both paused,fade 2s linear both paused}",
        r#"
        const list = target.getAnimations();
        list[0].currentTime = 200; list[1].currentTime = 800;
        target.style.animationName = 'fade';
        const remaining = target.getAnimations();
        check('last matched first', remaining.length === 1 && remaining[0] === list[1]);
        check('time retained with new duration', remaining[0].currentTime === 800);
        check('unmatched canceled', list[0].playState === 'idle');
        check('updated duration', getComputedStyle(target).opacity === '0.8');
    "#,
    );
}
