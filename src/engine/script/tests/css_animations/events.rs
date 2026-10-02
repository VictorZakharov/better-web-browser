use super::{assert_async_script, assert_script};

#[test]
fn start_iteration_and_end_events_are_real_bubbling_animation_events() {
    assert_async_script(
        "@keyframes fade{from{opacity:0}to{opacity:1}} #target{animation:fade 1s linear 3 both paused}",
        r#"
        const events = [];
        const parent = document.getElementById('parent');
        for (const type of ['animationstart','animationiteration','animationend'])
            parent.addEventListener(type, event => events.push([event.type,event.animationName,
                event.elapsedTime,event.target === target,event instanceof AnimationEvent,event.pseudoElement].join(':')));
        const animation = target.getAnimations()[0];
        animation.currentTime = 1250;
        animation.currentTime = 3000;
        requestAnimationFrame(() => {
            check('event phases', events.join('|') ===
                'animationstart:fade:0:true:true:|animationiteration:fade:1:true:true:|animationend:fade:3:true:true:');
            finish();
        });
    "#,
    );
}

#[test]
fn negative_delay_start_event_reports_skipped_active_time() {
    assert_async_script(
        "@keyframes fade{from{opacity:0}to{opacity:1}} #target{animation:fade 1s linear -250ms both paused}",
        r#"
        const events = [];
        target.addEventListener('animationstart', event => events.push(event.elapsedTime));
        const animation = target.getAnimations()[0];
        requestAnimationFrame(() => {
            check('negative delay elapsed', events.join(',') === '0.25');
            check('sample starts inside effect', getComputedStyle(target).opacity === '0.25');
            finish();
        });
    "#,
    );
}

#[test]
fn cancel_before_completion_reports_active_time_and_no_end() {
    assert_async_script(
        "@keyframes fade{from{opacity:0}to{opacity:1}} #target{animation:fade 1s linear both paused}",
        r#"
        const events = [];
        for (const type of ['animationcancel','animationend'])
            target.addEventListener(type, event => events.push(type+':'+event.elapsedTime));
        const animation = target.getAnimations()[0];
        animation.currentTime = 400;
        target.style.animationName = 'none';
        check('removed immediately', target.getAnimations().length === 0);
        requestAnimationFrame(() => { check('cancel elapsed', events.join('|') === 'animationcancel:0.4'); finish(); });
    "#,
    );
}

#[test]
fn zero_duration_starts_and_ends_once_without_iteration() {
    assert_async_script(
        "@keyframes fade{from{opacity:0}to{opacity:1}} #target{animation:fade 0s both paused}",
        r#"
        const events = [];
        for (const type of ['animationstart','animationiteration','animationend'])
            target.addEventListener(type, event => events.push(type+':'+event.elapsedTime));
        target.getAnimations();
        target.setAttribute('data-unrelated','one');
        getComputedStyle(target);
        requestAnimationFrame(() => { check('one phase pair', events.join('|') === 'animationstart:0|animationend:0'); finish(); });
    "#,
    );
}

#[test]
fn finished_animation_removal_does_not_generate_cancel() {
    assert_async_script(
        "@keyframes fade{from{opacity:0}to{opacity:1}} #target{animation:fade 1s linear both paused}",
        r#"
        const events = [];
        for (const type of ['animationcancel','animationend'])
            target.addEventListener(type, event => events.push(type));
        const animation = target.getAnimations()[0];
        animation.currentTime = 1000;
        target.style.animationName = 'none';
        target.getAnimations();
        requestAnimationFrame(() => { check('only completion', events.join('|') === 'animationend'); finish(); });
    "#,
    );
}

#[test]
fn animation_event_fields_are_readonly_and_have_the_expected_brand() {
    assert_script(
        "",
        r#"
        const event = new AnimationEvent('animationstart', {animationName:'spin',elapsedTime:.5,pseudoElement:'::before'});
        check('brand', Object.prototype.toString.call(event) === '[object AnimationEvent]');
        check('readonly', Object.getOwnPropertyDescriptor(event,'elapsedTime').writable === false);
        check('fields', event.animationName === 'spin' && event.elapsedTime === .5 && event.pseudoElement === '::before');
        let failed = false;
        try { new AnimationEvent('animationend', {elapsedTime:Infinity}); } catch(e) { failed = e instanceof TypeError; }
        check('finite WebIDL double', failed);
    "#,
    );
}
