use super::*;
mod advanced;

#[test]
fn class_change_interpolates_computed_color_transform_and_width() {
    let (dom, outcome) = execute_html(
        r#"<style>
            #target { display:block; width:20px; opacity:.2; background-color:rgb(0,0,0);
                transform:translateX(0px);
                transition: width 1s linear, opacity 1s linear,
                    background-color 1s linear, transform 1s linear; }
            #target.on { width:100px; opacity:1; background-color:rgb(200,0,0);
                transform:translateX(100px); }
        </style><body><div id=target></div><script>
            let now = 100;
            performance.now = () => now;
            const target = document.getElementById('target');
            const events = [];
            for (const type of ['transitionrun', 'transitionstart', 'transitionend'])
                target.addEventListener(type, event => {
                    if (event.propertyName === 'width') events.push(type + ':' + event.elapsedTime);
                });
            target.classList.add('on');
            const initial = [getComputedStyle(target).width,
                getComputedStyle(target).opacity];
            now = 600;
            requestAnimationFrame(() => {
                const middle = [getComputedStyle(target).width,
                    getComputedStyle(target).opacity,
                    getComputedStyle(target).backgroundColor,
                    getComputedStyle(target).transform];
                now = 1100;
                requestAnimationFrame(() => {
                    const final = [getComputedStyle(target).width,
                        getComputedStyle(target).opacity];
                    document.body.setAttribute('data-result',
                        [initial.join(','), middle.join(','), final.join(','), events.join(',')].join('|'));
                });
            });
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("body")
            .next()
            .unwrap()
            .attr("data-result")
            .as_deref(),
        Some(
            "20px,0.2|60px,0.6,rgb(100, 0, 0),translate(50px, 0px)|100px,1|transitionrun:0,transitionstart:0,transitionend:1"
        )
    );
}

#[test]
fn supported_border_and_flex_shorthands_select_their_longhands() {
    let (dom, outcome) = execute_html(
        r#"<style>
            #target { border-top:2px solid rgb(0,0,0); flex:0 1 auto;
                transition:border-top 1s linear, flex 1s linear; }
            #target.on { border-top:10px solid rgb(200,0,0); flex:2 0 auto; }
        </style><body><div id=target></div><script>
            let now = 100;
            performance.now = () => now;
            const target = document.getElementById('target');
            target.classList.add('on');
            now = 600;
            requestAnimationFrame(() => {
                const style = getComputedStyle(target);
                document.body.setAttribute('data-result', [style.borderTopWidth,
                    style.borderTopColor, style.flexGrow, style.flexShrink].join('|'));
            });
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("body")
            .next()
            .unwrap()
            .attr("data-result")
            .as_deref(),
        Some("6px|rgb(100, 0, 0)|1|0.5")
    );
}

#[test]
fn detaching_an_active_transition_cancels_without_a_late_end_event() {
    let (dom, outcome) = execute_html(
        r#"<body><div id=target style='opacity:0;transition:opacity 1s linear'></div><script>
            let now = 100;
            performance.now = () => now;
            const target = document.getElementById('target'), events = [];
            for (const type of ['transitionrun', 'transitionstart', 'transitioncancel', 'transitionend'])
                target.addEventListener(type, event => events.push(type + ':' + event.elapsedTime));
            target.style.opacity = '1';
            document.body.removeChild(target);
            now = 600;
            requestAnimationFrame(() => {
                now = 1100;
                requestAnimationFrame(() => document.body.setAttribute('data-result', events.join(',')));
            });
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("body")
            .next()
            .unwrap()
            .attr("data-result")
            .as_deref(),
        Some("transitionrun:0,transitionstart:0,transitioncancel:0.5")
    );
}

#[test]
fn reversed_transition_uses_spec_reversing_shortening_factor() {
    let (dom, outcome) = execute_html(
        r#"<body><div id=target style='opacity:0;transition:opacity 1s linear'></div><script>
            let now = 100;
            performance.now = () => now;
            const target = document.getElementById('target'), events = [];
            for (const type of ['transitionrun', 'transitionstart', 'transitioncancel', 'transitionend'])
                target.addEventListener(type, event => events.push(type + ':' + event.elapsedTime));
            target.style.opacity = '1';
            now = 600;
            requestAnimationFrame(() => {
                const middle = getComputedStyle(target).opacity;
                target.style.opacity = '0';
                const restarted = getComputedStyle(target).opacity;
                now = 1100;
                requestAnimationFrame(() => {
                    const secondMiddle = getComputedStyle(target).opacity;
                    now = 1600;
                    requestAnimationFrame(() => document.body.setAttribute('data-result',
                        [middle, restarted, secondMiddle, getComputedStyle(target).opacity,
                            events.join(',')].join('|')));
                });
            });
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("body")
            .next()
            .unwrap()
            .attr("data-result")
            .as_deref(),
        Some(
            "0.5|0.5|0|0|transitionrun:0,transitionstart:0,transitioncancel:0.5,transitionrun:0,transitionstart:0,transitionend:0.5"
        )
    );
}

#[test]
fn computed_transition_lists_repeat_without_exposing_unsupported_values() {
    let (dom, outcome) = execute_html(
        r#"<body><div id=target style='transition:opacity 250ms ease-in 50ms,
            transform 1s linear -100ms'></div><script>
            const target = document.getElementById('target');
            const style = getComputedStyle(target);
            const checks = [style.transitionProperty, style.transitionDuration,
                style.transitionTimingFunction, style.transitionDelay,
                CSS.supports('transition', 'opacity 1s ease-in'),
                CSS.supports('transition-duration', '-1s'),
                CSS.supports('transition-timing-function', 'cubic-bezier(.2, .8, .4, 1)')];
            document.body.setAttribute('data-result', checks.join('|'));
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("body")
            .next()
            .unwrap()
            .attr("data-result")
            .as_deref(),
        Some("opacity, transform|0.25s, 1s|ease-in, linear|0.05s, -0.1s|true|false|true")
    );
}

#[test]
fn ordinary_attribute_changes_without_transition_rules_do_not_install_animation_styles() {
    let (dom, outcome) = execute_html(
        r#"<style>.on {opacity:.5}</style><body><div id=target></div><script>
            const target = document.getElementById('target');
            target.className = 'on';
            target.style.opacity = '.7';
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    let target = dom.elements_named("div").next().unwrap();
    assert_eq!(target.animation_style(), None);
}

#[test]
fn zero_duration_waits_for_positive_delay_and_queues_lifecycle_events() {
    let (dom, outcome) = execute_html(
        r#"<body><div id=target style='opacity:0;transition:opacity 0s linear 250ms'></div><script>
            let now = 100;
            performance.now = () => now;
            const target = document.getElementById('target'), events = [];
            for (const type of ['transitionrun', 'transitionstart', 'transitionend'])
                target.addEventListener(type, event => events.push(type + ':' + event.elapsedTime));
            target.style.opacity = '1';
            const immediate = [getComputedStyle(target).opacity, events.length];
            requestAnimationFrame(() => {
                const afterRun = events.join(',');
                now = 349;
                requestAnimationFrame(() => {
                    const beforeDelay = getComputedStyle(target).opacity;
                    now = 350;
                    requestAnimationFrame(() => document.body.setAttribute('data-result',
                        [immediate.join(','), afterRun, beforeDelay,
                            getComputedStyle(target).opacity, events.join(',')].join('|')));
                });
            });
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("body")
            .next()
            .unwrap()
            .attr("data-result")
            .as_deref(),
        Some("0,0|transitionrun:0|0|1|transitionrun:0,transitionstart:0,transitionend:0")
    );
}

#[test]
fn active_transition_target_cap_is_bounded_and_other_changes_still_apply() {
    let (dom, outcome) = execute_html(
        r#"<body><script>
            let now = 100;
            performance.now = () => now;
            const targets = [];
            for (let index = 0; index < 65; index++) {
                const target = document.createElement('div');
                target.setAttribute('style', 'opacity:0;transition:opacity 1s linear');
                document.body.appendChild(target);
                targets.push(target);
            }
            for (const target of targets) target.style.opacity = '1';
            document.body.setAttribute('data-result',
                [getComputedStyle(targets[0]).opacity,
                    getComputedStyle(targets[63]).opacity,
                    getComputedStyle(targets[64]).opacity].join(','));
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("body")
            .next()
            .unwrap()
            .attr("data-result")
            .as_deref(),
        Some("0,0,1")
    );
}

#[test]
fn lifecycle_queue_is_bounded_and_keeps_accepted_event_order() {
    let (dom, outcome) = execute_html(
        r#"<body><div id=target style='opacity:0;transition:opacity 1s linear'></div><script>
            let now = 100;
            performance.now = () => now;
            const target = document.getElementById('target'), events = [];
            for (const type of ['transitionrun', 'transitionstart', 'transitioncancel'])
                target.addEventListener(type, event => events.push(event.type));
            for (let index = 0; index < 1370; index++)
                target.style.opacity = index % 2 === 0 ? '1' : '0';
            requestAnimationFrame(() => document.body.setAttribute('data-result',
                [events.length, events.slice(0, 3).join(','),
                    events[events.length - 1]].join('|')));
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("body")
            .next()
            .unwrap()
            .attr("data-result")
            .as_deref(),
        Some("2048|transitionrun,transitionstart,transitioncancel|transitionstart")
    );
}

#[test]
fn inline_transition_after_css_comment_starts_on_style_change() {
    let (dom, outcome) = execute_html(
        r#"<body><div id=target style='opacity:0;/* note */transition:opacity 1s linear'></div><script>
            let now = 100;
            performance.now = () => now;
            const target = document.getElementById('target');
            target.style.opacity = '1';
            now = 600;
            requestAnimationFrame(() => document.body.setAttribute('data-result',
                getComputedStyle(target).opacity));
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("body")
            .next()
            .unwrap()
            .attr("data-result")
            .as_deref(),
        Some("0.5")
    );
}

#[test]
fn unsupported_calc_interpolation_preserves_cssom_instead_of_fabricating_zero() {
    let (dom, outcome) = execute_html(
        r#"<body><div id=target style='width:calc(10px + 10%);transition:width 1s linear'></div><script>
            const target = document.getElementById('target');
            const before = getComputedStyle(target).width;
            target.style.width = 'calc(20px + 10%)';
            document.body.setAttribute('data-result',
                before + '|' + getComputedStyle(target).width);
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("body")
            .next()
            .unwrap()
            .attr("data-result")
            .as_deref(),
        Some("calc(10px + 10%)|calc(20px + 10%)")
    );
}
