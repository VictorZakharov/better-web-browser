//! A timing calculation must affect the real CSSAnimation and native style,
//! not merely become a true CSS.supports() answer or stored authored string.
use super::{assert_async_script, assert_script};

#[test]
fn calculated_duration_delay_and_iterations_drive_real_animation_samples() {
    assert_script(
        "@keyframes fade{from{opacity:0}to{opacity:1}} #target{animation:fade min(2s,1000ms) linear calc(-250ms) sqrt(4) alternate both paused}",
        r#"
        const animation = target.getAnimations()[0];
        check('real CSSAnimation', animation instanceof CSSAnimation);
        const timing = animation.effect.getTiming();
        check('duration in milliseconds', timing.duration === 1000);
        check('negative delay', timing.delay === -250);
        check('calculated iterations', timing.iterations === 2);
        check('initial delay sample', getComputedStyle(target).opacity === '0.25');
        animation.currentTime = 250;
        check('forward sample', getComputedStyle(target).opacity === '0.5');
        animation.currentTime = 1000;
        check('alternate iteration', getComputedStyle(target).opacity === '0.75');
        animation.currentTime = 1750;
        check('terminal fill', getComputedStyle(target).opacity === '0');
        check('no author attribute rewrite', target.getAttribute('style') === null);
        check('computed duration', getComputedStyle(target).animationDuration === '1s');
        check('computed delay', getComputedStyle(target).animationDelay === '-0.25s');
        "#,
    );
}

#[test]
fn replacing_calculated_timing_keeps_animation_identity_and_updates_progress() {
    assert_script(
        "@keyframes fade{from{opacity:0}to{opacity:1}} #target{animation:fade 1s linear both paused}",
        r#"
        const animation = target.getAnimations()[0];
        animation.currentTime = 500;
        check('initial half', getComputedStyle(target).opacity === '0.5');
        target.style.animationDuration = 'calc(1s + 1000ms)';
        check('same animation', target.getAnimations()[0] === animation);
        check('new quarter', getComputedStyle(target).opacity === '0.25');
        check('current time retained', animation.currentTime === 500);
        target.style.animationDuration = 'calc(1s + 1px)';
        check('invalid value retained', getComputedStyle(target).animationDuration === '2s');
        target.style.animationIterationCount = 'calc(1 + .5)';
        check('updated iterations', animation.effect.getTiming().iterations === 1.5);
        animation.currentTime = 3000;
        check('fractional terminal sample', getComputedStyle(target).opacity === '0.5');
        "#,
    );
}

#[test]
fn zero_calculated_duration_and_count_are_not_invalid_declarations() {
    assert_script(
        "@keyframes fade{from{opacity:.2}to{opacity:.8}} #target{animation:fade calc(-1s) linear both paused}",
        r#"
        const animation = target.getAnimations()[0];
        check('clamped duration', animation.effect.getTiming().duration === 0);
        check('zero-duration terminal fill', getComputedStyle(target).opacity === '0.8');
        target.style.animationDuration = '1s';
        target.style.animationIterationCount = 'calc(-1)';
        check('zero iterations', animation.effect.getTiming().iterations === 0);
        check('zero active duration', animation.effect.getComputedTiming().activeDuration === 0);
        "#,
    );
}

#[test]
fn calculated_transition_timing_reaches_the_native_midpoint_and_finish() {
    assert_async_script(
        "#target{opacity:0;transition:opacity calc(500ms + .5s) linear calc(-250ms)} #target.on{opacity:1}",
        r#"
        let now = 100;
        performance.now = () => now;
        target.className = 'on';
        check('negative-delay initial sample', getComputedStyle(target).opacity === '0.25');
        now = 350;
        requestAnimationFrame(() => {
            check('native midpoint', getComputedStyle(target).opacity === '0.5');
            check('duration serialization', getComputedStyle(target).transitionDuration === '1s');
            check('delay serialization', getComputedStyle(target).transitionDelay === '-0.25s');
            now = 850;
            requestAnimationFrame(() => {
                check('completed sample', getComputedStyle(target).opacity === '1');
                finish();
            });
        });
        "#,
    );
}
