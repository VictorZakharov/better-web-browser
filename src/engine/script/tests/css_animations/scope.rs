//! A tree-scoped name belongs to its declaration, not necessarily its target.
use super::assert_script;

#[test]
fn host_rule_resolves_names_from_the_shadow_stylesheet() {
    assert_script(
        "@keyframes fade{from{opacity:0}to{opacity:1}}",
        r#"
        const root = target.attachShadow({mode:'open'});
        root.innerHTML = '<style>@keyframes fade{from{opacity:0}to{opacity:.4}} :host{animation:fade 1s linear both paused}</style>';
        const animation = target.getAnimations()[0];
        animation.currentTime = 500;
        check('host uses shadow name', getComputedStyle(target).opacity === '0.2');
        target.style.animation = 'fade 1s linear both paused';
        check('same name, new winning scope', getComputedStyle(target).opacity === '0.5');
        check('same animation identity', target.getAnimations()[0] === animation);
    "#,
    );
}

#[test]
fn slotted_rule_resolves_names_from_the_slot_tree() {
    assert_script(
        "@keyframes fade{from{opacity:0}to{opacity:1}}",
        r#"
        const root = document.getElementById('parent').attachShadow({mode:'open'});
        root.innerHTML = '<style>@keyframes fade{from{opacity:0}to{opacity:.6}} ::slotted(div){animation:fade 1s linear both paused}</style><slot></slot>';
        const animation = target.getAnimations()[0];
        check('assigned animation', animation instanceof CSSAnimation);
        animation.currentTime = 500;
        check('slot definition', getComputedStyle(target).opacity === '0.3');
    "#,
    );
}

#[test]
fn nested_shadow_scope_falls_back_to_the_enclosing_shadow() {
    assert_script(
        "@keyframes fade{from{opacity:0}to{opacity:1}}",
        r#"
        const outer = document.getElementById('parent').attachShadow({mode:'open'});
        outer.innerHTML = '<style>@keyframes fade{from{opacity:0}to{opacity:.8}}</style><section></section>';
        const inner = outer.querySelector('section').attachShadow({mode:'open'});
        inner.innerHTML = '<style>div{animation:fade 1s linear both paused}</style><div></div>';
        const node = inner.querySelector('div');
        node.getAnimations()[0].currentTime = 500;
        check('outer shadow fallback', getComputedStyle(node).opacity === '0.4');
    "#,
    );
}

#[test]
fn inherited_animation_name_preserves_its_original_scope() {
    assert_script(
        "@keyframes fade{from{opacity:0}to{opacity:1}}",
        r#"
        const root = target.attachShadow({mode:'open'});
        root.innerHTML = '<style>@keyframes fade{from{opacity:0}to{opacity:.6}} :host{animation-name:fade} div{animation:1s linear both paused;animation-name:inherit}</style><div></div>';
        const node = root.querySelector('div');
        node.getAnimations()[0].currentTime = 500;
        check('inherited shadow reference', getComputedStyle(node).opacity === '0.3');
    "#,
    );
}

#[test]
fn reverting_a_name_layer_restores_the_winning_scope() {
    assert_script(
        "@keyframes fade{from{opacity:0}to{opacity:1}}",
        r#"
        const root = target.attachShadow({mode:'open'});
        root.innerHTML = '<style>@keyframes fade{from{opacity:0}to{opacity:.4}} @layer base,override; @layer base{:host{animation:fade 1s linear both paused}} @layer override{:host{animation-name:revert-layer}}</style>';
        target.getAnimations()[0].currentTime = 500;
        check('reverted tree-scoped name', getComputedStyle(target).opacity === '0.2');
    "#,
    );
}
