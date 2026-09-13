use super::*;

fn check(script: &str) {
    let html = format!(
        "<body><output></output><script>function assert(v,m){{if(!v)throw Error(m)}}{script}</script>"
    );
    let (_, outcome) = execute_html(&html);
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
}

#[test]
fn checked_state_is_not_an_attribute_and_survives_cloning_and_reset() {
    check(
        r#"
        const form = document.createElement('form'); document.body.append(form);
        form.innerHTML = '<input type=checkbox checked><input type=radio name=g checked><input type=radio name=g checked>';
        const [box,a,b] = form.querySelectorAll('input');
        assert(box.checked && box.defaultChecked, 'parsed default');
        assert(!a.checked && b.checked, 'last parsed radio wins');
        box.checked = false;
        assert(box.defaultChecked && !box.matches(':checked'), 'IDL must not mutate attribute');
        box.removeAttribute('checked'); box.setAttribute('checked','');
        assert(!box.checked, 'dirty checkedness ignores attribute writes');
        const clone = box.cloneNode();
        assert(!clone.checked && clone.defaultChecked, 'clone copies dirty checkedness');
        clone.removeAttribute('checked'); clone.setAttribute('checked','');
        assert(!clone.checked, 'clone stays dirty');
        box.indeterminate = true;
        assert(box.matches(':indeterminate') && box.matches(':not(:checked)'), 'state selectors');
        form.reset();
        assert(box.checked && box.indeterminate && !a.checked && b.checked, 'reset checkedness only');
        box.removeAttribute('checked'); assert(!box.checked, 'reset clears dirty flag');
    "#,
    );
}

#[test]
fn canceled_activation_preserves_a_previously_checked_unnamed_radio() {
    check(
        r#"
        const radio = document.createElement('input'); radio.type='radio';
        document.body.append(radio); radio.checked=true;
        radio.onclick=e=>e.preventDefault(); radio.click();
        assert(radio.checked, 'canceling must restore a checked unnamed radio');
    "#,
    );
}

#[test]
fn radio_groups_follow_name_form_tree_and_insertion() {
    check(
        r#"
        document.body.insertAdjacentHTML('beforeend','<form id=f><input id=a type=radio name=g checked><input id=b type=radio name=h checked></form><input id=c type=radio name=g checked>');
        const a=document.getElementById('a'), b=document.getElementById('b'), c=document.getElementById('c');
        b.name='g'; assert(b.checked && !a.checked && c.checked, 'name regroup');
        c.setAttribute('form','f'); assert(c.checked && !b.checked, 'explicit form owner');
        const root=document.createElement('section'); const shadow=root.attachShadow({mode:'open'});
        shadow.innerHTML='<input type=radio name=g checked>'; document.body.append(root);
        assert(shadow.querySelector('input').checked && c.checked, 'shadow groups are separate');
        const clone=c.cloneNode(); document.body.append(clone);
        assert(clone.checked && !c.checked, 'connected checked clone wins');
        clone.type='checkbox'; c.checked=true;
        clone.type='radio'; assert(clone.checked && !c.checked, 'type regroup');
        const lone=document.createElement('input'); lone.type='radio';
        assert(lone.matches(':indeterminate'), 'unnamed radio');
    "#,
    );
}

#[test]
fn checkable_click_precedes_listeners_and_cancellation_rolls_back() {
    check(
        r#"
        document.body.insertAdjacentHTML('beforeend','<form><label><input type=checkbox><span>Toggle</span></label><input type=radio name=g checked><input type=radio name=g></form>');
        const [box,a,b]=document.querySelectorAll('input'), events=[];
        box.addEventListener('click',()=>events.push('click:'+box.checked));
        box.addEventListener('input',e=>events.push('input:'+e.bubbles+':'+e.composed));
        box.addEventListener('change',()=>events.push('change'));
        document.querySelector('span').click();
        assert(events.join('|')==='click:true|input:true:true|change', 'label click activation order: '+events);
        box.indeterminate=true;
        box.addEventListener('click',e=>e.preventDefault(),{once:true}); box.click();
        assert(box.checked && box.indeterminate && events.length===4, 'cancel restores checkbox');
        b.addEventListener('click',e=>{ assert(b.checked && !a.checked,'radio preactivation'); e.preventDefault(); },{once:true});
        b.click(); assert(a.checked && !b.checked,'cancel restores radio');
        box.disabled=true; box.click(); assert(events.length===4,'disabled click');
        const generic=new Event('click',{bubbles:true}); a.dispatchEvent(generic);
        assert(a.checked,'generic Event has no activation behavior');
    "#,
    );
}
