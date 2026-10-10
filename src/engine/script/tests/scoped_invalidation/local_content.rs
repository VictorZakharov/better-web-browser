use super::*;

fn unrelated() -> String {
    (0..300)
        .map(|index| format!("<article><div>Unrelated {index}</div></article>"))
        .collect()
}

#[test]
fn text_replacement_svg_paths_and_declarations_do_not_restyle_unrelated_has_subtrees() {
    let html = format!(
        r#"<style>
        body:has(.switch:checked) {{ color: red }}
        b:empty + span {{ font-size: 30px }}
        </style><input class=switch type=checkbox>
        <section><b id=n>old</b><span id=peer>tail</span><svg><path id=p d="M0 0L1 1"/></svg></section>
        {}<script>
        const n=document.getElementById('n'), p=document.getElementById('p');
        n.getBoundingClientRect(); const old=n.firstChild;
        n.textContent='new'; p.setAttribute('d','M1 1L2 2'); n.style.color='blue';
        n.getBoundingClientRect();
        if(n.firstChild===old || old.isConnected) throw Error('text identity was reused');
        n.textContent='';
        const peer=document.getElementById('peer');peer.getBoundingClientRect();
        if(getComputedStyle(peer).fontSize!=='30px')
            throw Error('local empty sibling selector was not refreshed');
        </script>"#,
        unrelated()
    );
    let stats = run_with_geometry(&html);
    assert!(stats.len() >= 3);
    assert!(stats[0].total_styles > 600);
    for stats in &stats[1..] {
        assert!(stats.recomputed_styles < 20, "{stats:?}");
    }
}

#[test]
fn text_dependent_has_results_and_auto_direction_still_match_fresh_global_cascade() {
    for (selector, initial, replacement) in [
        ("body:has(b:empty)", "abc", ""),
        ("section:dir(rtl)", "abc", "אבג"),
    ] {
        let html = format!(
            r#"<style>{selector} {{ font-size: 40px }}</style>
            <section dir=auto><div><b id=n>{initial}</b></div></section>{}<script>
            const n=document.getElementById('n');n.getBoundingClientRect();
            n.textContent='{replacement}';n.getBoundingClientRect();
            if(getComputedStyle(n).fontSize!=='40px') throw Error('text-sensitive selector was stale');
            </script>"#,
            unrelated()
        );
        let stats = run_with_geometry(&html);
        assert!(stats.len() >= 2);
        assert!(
            stats[1].recomputed_styles > 600,
            "{selector}: {:?}",
            stats[1]
        );
    }
}

#[test]
fn relative_graphics_attribute_results_keep_their_global_refresh() {
    let html = format!(
        r#"<style>body:has([d]) {{ font-size: 40px }}</style>
        <section><b id=n>label</b><svg><path id=p /></svg></section>{}<script>
        const n=document.getElementById('n');n.getBoundingClientRect();
        n.textContent='updated';document.getElementById('p').setAttribute('d','M0 0L1 1');
        n.getBoundingClientRect();
        if(getComputedStyle(n).fontSize!=='40px') throw Error('relative attribute was stale');
        </script>"#,
        unrelated()
    );
    let stats = run_with_geometry(&html);
    assert!(stats[1].recomputed_styles > 600, "{:?}", stats[1]);
}

#[test]
fn text_replacement_refreshes_ancestor_form_and_fieldset_validity_without_has() {
    for (tag, extra) in [
        ("form", ""),
        ("fieldset", ""),
        ("form", "body:has(.active) {font-size:40px}"),
        ("fieldset", "body:has(.active) {font-size:40px}"),
    ] {
        let html = format!(
            r#"<style>{extra} {tag}:invalid {{ color: red }}</style>
            <{tag} id=group><section><div><textarea id=n required>old</textarea></div></section></{tag}>
            {}<script>
            const n=document.getElementById('n'), group=document.getElementById('group');
            n.getBoundingClientRect();n.textContent='';n.getBoundingClientRect();
            if(getComputedStyle(group).color!=='rgb(255, 0, 0)') throw Error('aggregate validity was stale');
            </script>"#,
            unrelated()
        );
        let stats = run_with_geometry(&html);
        assert!(stats[0].total_styles > 600);
        assert!(stats[1].recomputed_styles < 20, "{tag}: {:?}", stats[1]);
    }
}

#[test]
fn replacing_element_children_and_style_text_never_gets_a_text_only_shortcut() {
    let html = format!(
        r#"<style id=sheet>body:has(.active) {{ font-size: 40px }}</style>
        <section id=n><b class=active>old</b></section>{}<script>
        const n=document.getElementById('n');n.getBoundingClientRect();
        n.textContent='new';n.getBoundingClientRect();
        if(getComputedStyle(n).fontSize==='40px') throw Error('removed element membership was stale');
        document.getElementById('sheet').textContent='section {{ font-size: 35px }}';
        n.getBoundingClientRect();
        if(getComputedStyle(n).fontSize!=='35px') throw Error('stylesheet replacement was stale');
        </script>"#,
        unrelated()
    );
    let stats = run_with_geometry(&html);
    assert!(stats[1].recomputed_styles > 600);
    assert!(stats.last().unwrap().recomputed_styles > 600);
}

#[test]
fn scoped_and_shadow_rules_do_not_use_the_ordinary_selector_dependency_proof() {
    for content in [
        r#"<style>@scope (.shell) { b {color: red} }</style>
        <section class=shell><b id=n>old</b></section>"#,
        r#"<section id=host><template shadowrootmode=open>
        <style>::slotted(b) {color: red}</style><slot></slot>
        </template><b id=n>old</b></section>"#,
    ] {
        let html = format!(
            r#"{content}{}<script>
            const n=document.getElementById('n');n.getBoundingClientRect();
            n.textContent='updated';n.getBoundingClientRect();
            if(getComputedStyle(n).color!=='rgb(255, 0, 0)') throw Error('scoped style was stale');
            </script>"#,
            unrelated()
        );
        let stats = run_with_geometry(&html);
        assert!(stats[1].recomputed_styles > 600, "{:?}", stats[1]);
    }
}
