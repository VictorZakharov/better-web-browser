//! Behavior-level CSS Animation tests: the stylesheet must change native computed style.
use super::*;

mod cssom;
mod easing_math;
mod events;
mod layers;
mod lifecycle;
mod limits;
mod math;
mod ordering;
mod overrides;
mod scope;
mod stylesheet_changes;
mod timing;
mod transform_math;
mod values;

fn assert_script(styles: &str, script: &str) {
    assert_async_script(styles, &format!("{script}\nfinish();"));
}

pub(super) fn assert_async_script(styles: &str, script: &str) {
    let (dom, outcome) = execute_html(&format!(
        "<style>{styles}</style><body><main id=parent><div id=target></div></main><script>
        const failures = [];
        const check = (name, condition) => {{ if (!condition) failures.push(name); }};
        const finish = () => document.body.setAttribute('data-result', failures.join(',') || 'pass');
        const target = document.getElementById('target');
        {script}
        </script></body>"
    ));
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("body")
            .next()
            .unwrap()
            .attr("data-result")
            .as_deref(),
        Some("pass")
    );
}

#[test]
fn stylesheet_animation_samples_native_style_without_touching_author_attributes() {
    assert_script(
        "@keyframes fade {from{opacity:0} to{opacity:1}}
        #target{animation:fade 1s linear both}",
        r#"
        const animation = target.getAnimations()[0];
        check('CSSAnimation', animation instanceof CSSAnimation && animation instanceof Animation);
        check('name', animation.animationName === 'fade');
        animation.pause(); animation.currentTime = 500;
        check('native sample', getComputedStyle(target).opacity === '0.5');
        check('not inline', target.getAttribute('style') === null);
        check('effect', animation.effect.getKeyframes().length === 2);
        check('enumerated', document.getAnimations().includes(animation));
        animation.cancel();
        check('underlying restored', getComputedStyle(target).opacity === '1');
        check('cancel not restarted', target.getAnimations().length === 0);
        "#,
    );
}

#[test]
fn css_animation_names_remain_case_sensitive() {
    assert_script(
        "@keyframes Fade {from{opacity:0} to{opacity:.4}}
        @keyframes fade {from{opacity:0} to{opacity:1}}
        #target{animation:Fade 1s linear both}",
        r#"
        const animation = target.getAnimations()[0];
        animation.pause(); animation.currentTime = 500;
        check('uppercase rule', getComputedStyle(target).opacity === '0.2');
        target.style.animationName = 'fade';
        const second = target.getAnimations()[0];
        check('new identity', second !== animation && second.animationName === 'fade');
        second.pause(); second.currentTime = 500;
        check('lowercase rule', getComputedStyle(target).opacity === '0.5');
        "#,
    );
}

#[test]
fn negative_delay_and_alternate_iteration_use_existing_timeline() {
    assert_script(
        "@keyframes fade{from{opacity:0}to{opacity:1}}
        #target{animation:fade 1s linear -.25s 2 alternate both paused}",
        r#"
        const animation = target.getAnimations()[0];
        check('paused CSS state', animation.playState === 'paused');
        animation.currentTime = 0;
        check('negative delay', getComputedStyle(target).opacity === '0.25');
        animation.currentTime = 1000;
        check('alternate', getComputedStyle(target).opacity === '0.75');
        animation.currentTime = 1750;
        check('alternate final', getComputedStyle(target).opacity === '0');
        "#,
    );
}

#[test]
fn missing_endpoints_sample_the_underlying_computed_value() {
    assert_script(
        "@keyframes partial{50%{opacity:1}}
        #target{opacity:.2;animation:partial 1s linear both paused}",
        r#"
        const animation = target.getAnimations()[0];
        animation.currentTime = 250;
        check('implicit first', getComputedStyle(target).opacity === '0.6');
        animation.currentTime = 750;
        check('implicit last', getComputedStyle(target).opacity === '0.6');
        "#,
    );
}

#[test]
fn duplicate_offsets_merge_properties_and_ignore_important() {
    assert_script(
        "@keyframes fade{from{opacity:0;color:red}0%{color:blue}
        to{opacity:1;color:white!important}}
        #target{color:black;animation:fade 1s linear both paused}",
        r#"
        const animation = target.getAnimations()[0];
        const frames = animation.effect.getKeyframes();
        check('merged length', frames.length === 2);
        check('merged declaration', frames[0].color === 'rgb(0, 0, 255)');
        check('ignored important', !('color' in frames[1]));
        animation.currentTime = 500;
        check('opacity', getComputedStyle(target).opacity === '0.5');
        "#,
    );
}

#[test]
fn important_author_values_remain_above_css_animation_origin() {
    assert_script(
        "@keyframes fade{from{opacity:0}to{opacity:1}}
        #target{opacity:.3!important;animation:fade 1s linear both paused}",
        r#"
        const animation = target.getAnimations()[0];
        animation.currentTime = 500;
        check('important wins', getComputedStyle(target).opacity === '0.3');
        target.style.opacity = '.2';
        check('stylesheet important wins', getComputedStyle(target).opacity === '0.3');
        "#,
    );
}

#[test]
fn animation_list_order_wins_without_restarting_matching_names() {
    assert_script(
        "@keyframes low{from{opacity:0}to{opacity:.4}}
        @keyframes high{from{opacity:0}to{opacity:1}}
        #target{animation:low 1s linear both paused, high 1s linear both paused}",
        r#"
        const original = target.getAnimations();
        original.forEach(animation => animation.currentTime = 500);
        check('last name wins', getComputedStyle(target).opacity === '0.5');
        target.style.animationName = 'high, low';
        const reordered = target.getAnimations();
        check('identity preserved', reordered.includes(original[0]) && reordered.includes(original[1]));
        check('time preserved', reordered.every(animation => animation.currentTime === 500));
        check('new order wins', getComputedStyle(target).opacity === '0.2');
        "#,
    );
}

#[test]
fn unrelated_attribute_mutation_does_not_restart_animation() {
    assert_script(
        "@keyframes fade{from{opacity:0}to{opacity:1}}
        #target{animation:fade 1s linear both paused}",
        r#"
        const animation = target.getAnimations()[0];
        animation.currentTime = 750;
        target.setAttribute('data-other', 'x');
        check('same animation', target.getAnimations()[0] === animation);
        check('same time', animation.currentTime === 750);
        check('same sample', getComputedStyle(target).opacity === '0.75');
        "#,
    );
}

#[test]
fn constructor_and_animation_event_interfaces_are_not_presence_only() {
    assert_script(
        "",
        r#"
        let illegal = false;
        try { new CSSAnimation(); } catch(error) { illegal = error instanceof TypeError; }
        check('illegal constructor', illegal);
        const event = new AnimationEvent('animationend',
            {bubbles:true, animationName:'fade', elapsedTime:.5, pseudoElement:'::before'});
        check('event fields', event.animationName === 'fade' && event.elapsedTime === .5 && event.pseudoElement === '::before');
        check('event inheritance', event instanceof Event && event.bubbles);
        let invalid = false;
        try { new AnimationEvent('animationend', {elapsedTime:Infinity}); }
        catch(error) { invalid = error instanceof TypeError; }
        check('finite elapsed', invalid);
        "#,
    );
}
