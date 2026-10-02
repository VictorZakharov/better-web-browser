use super::super::css_animations::assert_async_script;

#[test]
fn ancestor_toggle_discovers_initial_inline_only_transition() {
    assert_async_script(
        "#target{opacity:0} main.on #target{opacity:1}",
        r#"
        let now = 100; performance.now = () => now;
        target.style.transition = 'opacity 1s linear';
        document.getElementById('parent').className = 'on'; now = 600;
        requestAnimationFrame(() => {
            check('inline descendant sample', getComputedStyle(target).opacity === '0.5');
            now = 1100; requestAnimationFrame(finish);
        });
    "#,
    );
}

#[test]
fn parsed_inline_transition_is_indexed_when_a_fragment_is_inserted() {
    assert_async_script(
        "section{opacity:0} main.on section{opacity:1}",
        r#"
        let now = 100; performance.now = () => now;
        const parent = document.getElementById('parent');
        parent.insertAdjacentHTML('beforeend','<section style="transition:opacity 1s linear"></section>');
        const child = parent.querySelector('section');
        parent.className = 'on'; now = 600;
        requestAnimationFrame(() => {
            check('inserted inline descendant', getComputedStyle(child).opacity === '0.5');
            now = 1100; requestAnimationFrame(finish);
        });
    "#,
    );
}

#[test]
fn ancestor_class_change_transitions_a_descendants_computed_style() {
    assert_async_script(
        "#target{opacity:0;transition:opacity 1s linear} main.on #target{opacity:1}",
        r#"
        let now = 100; performance.now = () => now;
        document.getElementById('parent').className = 'on';
        check('before-change retained', getComputedStyle(target).opacity === '0');
        now = 600;
        requestAnimationFrame(() => {
            check('descendant interpolated', getComputedStyle(target).opacity === '0.5');
            now = 1100;
            requestAnimationFrame(() => { check('final value', getComputedStyle(target).opacity === '1'); finish(); });
        });
    "#,
    );
}

#[test]
fn transitions_outrank_important_author_declarations_during_their_active_phase() {
    assert_async_script(
        "#target{opacity:0!important;transition:opacity 1s linear} #target.on{opacity:1!important}",
        r#"
        let now = 100; performance.now = () => now;
        target.className = 'on'; now = 600;
        requestAnimationFrame(() => {
            check('transition origin above important', getComputedStyle(target).opacity === '0.5');
            now = 1100;
            requestAnimationFrame(() => { check('important final value', getComputedStyle(target).opacity === '1'); finish(); });
        });
    "#,
    );
}

#[test]
fn function_commas_do_not_split_the_timing_function_list() {
    assert_async_script(
        "#target{opacity:0;width:0px;transition:opacity 1s cubic-bezier(0,0,1,1),width 1s steps(4,end)} #target.on{opacity:1;width:100px}",
        r#"
        let now = 100; performance.now = () => now;
        target.className = 'on'; now = 700;
        requestAnimationFrame(() => {
            check('bezier separate item', Math.abs(Number(getComputedStyle(target).opacity)-.6)<.001);
            check('steps separate item', getComputedStyle(target).width === '50px');
            now = 1100;
            requestAnimationFrame(finish);
        });
    "#,
    );
}

#[test]
fn piecewise_linear_css_easing_uses_the_native_supported_parser() {
    assert_async_script(
        "#target{opacity:0;transition:opacity 1s linear(0, .8 50%, 1)} #target.on{opacity:1}",
        r#"
        let now = 100; performance.now = () => now;
        check('real supports', CSS.supports('transition-timing-function','linear(0,.8 50%,1)'));
        target.className = 'on'; now = 600;
        requestAnimationFrame(() => {
            check('piecewise sample', getComputedStyle(target).opacity === '0.8');
            now = 1100; requestAnimationFrame(finish);
        });
    "#,
    );
}

#[test]
fn reversing_twice_uses_the_previous_reversing_shortening_factor() {
    assert_async_script(
        "#target{opacity:0;transition:opacity 1s linear} #target.on{opacity:1}",
        r#"
        let now = 100; performance.now = () => now;
        target.className = 'on'; now = 600;
        requestAnimationFrame(() => {
            check('first half', getComputedStyle(target).opacity === '0.5');
            target.className = ''; now = 850;
            requestAnimationFrame(() => {
                check('reverse half', getComputedStyle(target).opacity === '0.25');
                target.className = 'on'; now = 1225;
                requestAnimationFrame(() => {
                    check('second reversal', getComputedStyle(target).opacity === '0.625');
                    now = 1600; requestAnimationFrame(() => {
                        check('shortened second duration', getComputedStyle(target).opacity === '1'); finish();
                    });
                });
            });
        });
    "#,
    );
}

#[test]
fn hiding_an_ancestor_cancels_without_a_late_end_event() {
    assert_async_script(
        "#target{opacity:0;transition:opacity 1s linear} #target.on{opacity:1}",
        r#"
        let now = 100; performance.now = () => now;
        const events = [];
        for (const type of ['transitioncancel','transitionend'])
            target.addEventListener(type, event => events.push(type+':'+event.elapsedTime));
        target.className = 'on'; now = 400;
        document.getElementById('parent').style.display = 'none';
        requestAnimationFrame(() => {
            now = 2000;
            requestAnimationFrame(() => { check('ancestor cancellation', events.join('|') === 'transitioncancel:0.3'); finish(); });
        });
    "#,
    );
}
