use super::assert_script;

#[test]
fn unknown_name_starts_when_keyframes_become_available() {
    assert_script(
        "#target{animation:late 1s linear both paused}",
        r#"
        check('unknown absent', target.getAnimations().length === 0);
        document.styleSheets[0].insertRule('@keyframes late{from{opacity:0}to{opacity:1}}', 0);
        const animation = target.getAnimations()[0];
        check('now available', animation instanceof CSSAnimation && animation.currentTime === 0);
        animation.currentTime = 500;
        check('native sample', getComputedStyle(target).opacity === '0.5');
        "#,
    );
}

#[test]
fn hidden_ancestor_cancels_and_showing_it_creates_a_new_animation() {
    assert_script(
        "@keyframes fade{from{opacity:0}to{opacity:1}}
        #target{animation:fade 1s linear both paused}",
        r#"
        const first = target.getAnimations()[0];
        first.currentTime = 500;
        document.getElementById('parent').style.display = 'none';
        check('hidden canceled', target.getAnimations().length === 0 && first.playState === 'idle');
        document.getElementById('parent').style.display = 'block';
        const second = target.getAnimations()[0];
        check('new animation', second !== first && second.currentTime === 0);
        "#,
    );
}

#[test]
fn disconnected_target_does_not_keep_an_animation_running() {
    assert_script(
        "@keyframes fade{from{opacity:0}to{opacity:1}}
        #target{animation:fade 1s linear both paused}",
        r#"
        const animation = target.getAnimations()[0];
        target.remove();
        check('disconnected removed', document.getAnimations().length === 0);
        check('canceled', animation.playState === 'idle');
        document.getElementById('parent').append(target);
        check('reattached restarts', target.getAnimations()[0] !== animation);
        "#,
    );
}

#[test]
fn class_and_ancestor_selector_changes_start_and_stop_effects() {
    assert_script(
        "@keyframes fade{from{opacity:0}to{opacity:1}}
        main.on #target{animation:fade 1s linear both paused}",
        r#"
        check('initially absent', target.getAnimations().length === 0);
        document.getElementById('parent').className = 'on';
        check('ancestor starts', target.getAnimations().length === 1);
        document.getElementById('parent').className = '';
        check('ancestor removes', target.getAnimations().length === 0);
        "#,
    );
}

#[test]
fn shadow_tree_names_override_outer_keyframes_without_leaking_to_siblings() {
    assert_script(
        "@keyframes fade{from{opacity:0}to{opacity:1}}
        #target{animation:fade 1s linear both paused}",
        r#"
        const root = document.getElementById('parent').attachShadow({mode:'open'});
        root.innerHTML = '<style>@keyframes fade{from{opacity:0}to{opacity:.4}} div{animation:fade 1s linear both paused}</style><div id=inner></div>';
        const inner = root.getElementById('inner');
        const animation = inner.getAnimations()[0];
        animation.currentTime = 500;
        check('shadow definition', getComputedStyle(inner).opacity === '0.2');
        check('uncomposed light child removed', target.getAnimations().length === 0);
        "#,
    );
}

#[test]
fn all_inherit_is_not_pruned_by_the_animation_candidate_filter() {
    for declaration in ["#target{all:inherit}", ""] {
        assert_script(
            &format!(
                "@keyframes fade{{from{{opacity:0}}to{{opacity:1}}}} main{{animation:fade 1s linear both paused}} {declaration}"
            ),
            if declaration.is_empty() {
                r#"
                target.style.setProperty('all','inherit');
                const animation = target.getAnimations()[0];
                check('inline all discovered', animation instanceof CSSAnimation);
                animation.currentTime = 500;
                check('inline inheritance sampled', getComputedStyle(target).opacity === '0.5');
                "#
            } else {
                r#"
                const animation = target.getAnimations()[0];
                check('stylesheet all discovered', animation instanceof CSSAnimation);
                animation.currentTime = 500;
                check('stylesheet inheritance sampled', getComputedStyle(target).opacity === '0.5');
                "#
            },
        );
    }
}
