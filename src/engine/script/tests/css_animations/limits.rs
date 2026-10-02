use super::assert_script;

#[test]
fn stylesheet_frames_above_script_limit_do_not_abort_the_realm() {
    assert_script(
        "",
        r#"
        const sheet = document.styleSheets[0];
        const frames = Array.from({length:101},(_,index)=>index+'%{opacity:'+index/100+'}').join('');
        sheet.insertRule('@keyframes many{'+frames+'}',0);
        target.style.animation = 'many 1s linear both paused';
        const animation = target.getAnimations()[0];
        check('bounded native frames available', animation.effect.getKeyframes().length === 101);
        animation.currentTime = 500;
        check('middle sample', getComputedStyle(target).opacity === '0.5');
        let rejected = false;
        try { target.animate(Array.from({length:65},()=>({opacity:1})),1000); }
        catch(e) { rejected = e.name === 'NotSupportedError'; }
        check('public resource contract unchanged', rejected);
    "#,
    );
}

#[test]
fn animation_target_cap_does_not_prevent_other_dom_work() {
    assert_script(
        "@keyframes fade{from{opacity:0}to{opacity:1}} .animated{animation:fade 1s linear both paused}",
        r#"
        for (let index=0;index<80;index++) {
            const node = document.createElement('div'); node.className='animated';
            document.getElementById('parent').append(node);
        }
        check('target cap', document.getAnimations().length === 64);
        target.textContent = 'still usable';
        check('DOM remains usable', target.textContent === 'still usable');
        const first = document.getAnimations()[0]; first.currentTime=500;
        check('accepted effect samples', getComputedStyle(first.effect.target).opacity === '0.5');
    "#,
    );
}

#[test]
fn per_target_animation_cap_bounds_name_lists() {
    assert_script(
        "@keyframes fade{from{opacity:0}to{opacity:1}}",
        r#"
        target.style.animationName = Array(30).fill('fade').join(',');
        target.style.animationDuration = '1s'; target.style.animationFillMode = 'both';
        target.style.animationPlayState = 'paused';
        check('effect count bounded', target.getAnimations().length === 16);
        check('CSSOM retains author list', getComputedStyle(target).animationName.split(',').length === 30);
    "#,
    );
}

#[test]
fn animation_sampling_does_not_change_author_style_or_attributes() {
    assert_script(
        "@keyframes fade{from{opacity:0}to{opacity:1}} #target{animation:fade 1s linear both paused}",
        r#"
        target.style.color = 'red';
        const source = target.getAttribute('style');
        const animation = target.getAnimations()[0];
        for (let index=0;index<100;index++) animation.currentTime=index*10;
        check('author style unchanged', target.getAttribute('style') === source);
        check('native presentation visible', getComputedStyle(target).opacity === '0.99');
        check('style CSSOM still author', target.style.opacity === '');
    "#,
    );
}
