use super::assert_script;

#[test]
fn positive_delay_uses_backwards_fill_then_enters_active_interval() {
    assert_script(
        "@keyframes fade{from{opacity:.2}to{opacity:.8}} #target{opacity:.9;animation:fade 1s linear 250ms backwards paused}",
        r#"
        const animation = target.getAnimations()[0];
        check('backwards fill', getComputedStyle(target).opacity === '0.2');
        animation.currentTime = 750;
        check('active delay removed', getComputedStyle(target).opacity === '0.5');
        animation.currentTime = 1250;
        check('no forwards fill', getComputedStyle(target).opacity === '0.9');
    "#,
    );
}

#[test]
fn reverse_backwards_fill_samples_the_last_keyframe() {
    assert_script(
        "@keyframes fade{from{opacity:.2}to{opacity:.8}} #target{animation:fade 1s linear 250ms reverse both paused}",
        r#"
        const animation = target.getAnimations()[0];
        check('reverse before', getComputedStyle(target).opacity === '0.8');
        animation.currentTime = 750;
        check('reverse middle', getComputedStyle(target).opacity === '0.5');
        animation.currentTime = 1250;
        check('reverse end', getComputedStyle(target).opacity === '0.2');
    "#,
    );
}

#[test]
fn fractional_iteration_count_samples_partial_terminal_iteration() {
    assert_script(
        "@keyframes fade{from{opacity:0}to{opacity:1}} #target{animation:fade 1s linear 2.25 both paused}",
        r#"
        const animation = target.getAnimations()[0]; animation.currentTime = 2250;
        check('fractional fill', getComputedStyle(target).opacity === '0.25');
        check('active duration', animation.effect.getComputedTiming().activeDuration === 2250);
    "#,
    );
}

#[test]
fn infinite_iterations_remain_active_without_a_fabricated_finish() {
    assert_script(
        "@keyframes fade{from{opacity:0}to{opacity:1}} #target{animation:fade 1s linear infinite alternate both paused}",
        r#"
        const animation = target.getAnimations()[0]; animation.currentTime = 10250;
        check('late repeated sample', getComputedStyle(target).opacity === '0.25');
        check('infinite duration', animation.effect.getComputedTiming().activeDuration === Infinity);
        let failed = false; try { animation.finish(); } catch(e) { failed = e.name === 'InvalidStateError'; }
        check('cannot finish infinity', failed);
    "#,
    );
}

#[test]
fn changing_duration_updates_progress_without_resetting_current_time() {
    assert_script(
        "@keyframes fade{from{opacity:0}to{opacity:1}} #target{animation:fade 1s linear both paused}",
        r#"
        const animation = target.getAnimations()[0]; animation.currentTime = 500;
        target.style.animationDuration = '2s';
        check('identity', target.getAnimations()[0] === animation);
        check('retained time', animation.currentTime === 500);
        check('new duration sample', getComputedStyle(target).opacity === '0.25');
    "#,
    );
}

#[test]
fn paused_by_script_is_not_restarted_by_an_unrelated_css_change() {
    assert_script(
        "@keyframes fade{from{opacity:0}to{opacity:1}} #target{animation:fade 1s linear both}",
        r#"
        const animation = target.getAnimations()[0]; animation.pause(); animation.currentTime = 500;
        target.style.color = 'red'; target.getAnimations();
        check('script pause retained', animation.playState === 'paused' && animation.currentTime === 500);
        check('sample retained', getComputedStyle(target).opacity === '0.5');
    "#,
    );
}

#[test]
fn per_keyframe_easing_overrides_global_easing_for_its_interval() {
    assert_script(
        "@keyframes fade{from{opacity:0;animation-timing-function:steps(2,end)}50%{opacity:.5;animation-timing-function:linear}to{opacity:1}} #target{animation:fade 1s ease-in both paused}",
        r#"
        const animation = target.getAnimations()[0]; animation.currentTime = 200;
        check('first keyframe steps', getComputedStyle(target).opacity === '0');
        animation.currentTime = 750;
        check('next interval linear', getComputedStyle(target).opacity === '0.75');
        check('last easing kept in CSSOM', animation.effect.getKeyframes()[0].easing === 'steps(2,end)');
    "#,
    );
}

#[test]
fn settings_lists_repeat_to_the_number_of_animation_names() {
    assert_script(
        "@keyframes a{from{opacity:0}to{opacity:1}} @keyframes b{from{width:0px}to{width:100px}} #target{animation-name:a,b;animation-duration:1s,2s;animation-timing-function:linear;animation-fill-mode:both;animation-play-state:paused}",
        r#"
        const list = target.getAnimations(); list[0].currentTime = 500; list[1].currentTime = 500;
        check('durations independent', getComputedStyle(target).opacity === '0.5' && getComputedStyle(target).width === '25px');
        check('computed list not expanded', getComputedStyle(target).animationTimingFunction === 'linear');
        check('timing repeated in effect', list[1].effect.getKeyframes()[0].easing === 'linear');
    "#,
    );
}

#[test]
fn linear_easing_stops_are_sampled_per_css_keyframe_interval() {
    assert_script(
        "@keyframes fade{from{opacity:0}to{opacity:1}} #target{animation:fade 1s linear(0,.8 50%,1) both paused}",
        r#"
        const animation = target.getAnimations()[0]; animation.currentTime = 500;
        check('real piecewise easing', getComputedStyle(target).opacity === '0.8');
        animation.currentTime = 750;
        check('second linear interval', getComputedStyle(target).opacity === '0.9');
    "#,
    );
}
