//! CSS direction is computed state, independent of HTML's :dir semantics.
use super::*;

#[test]
fn text_direction_inherits_and_all_keywords_do_not_reset_it() {
    let (_, outcome) = execute_html(
        r#"<style>
        main{direction:rtl;text-align:end}
        .initial{direction:initial}.inherit{direction:inherit}.unset{direction:unset}
        .revert{direction:ltr;direction:revert}
        .allInitial{direction:rtl;all:initial}.allInherit{direction:ltr;all:inherit}
        .allUnset{direction:ltr;all:unset}.allRevert{direction:rtl;all:revert}
        .allLayer{direction:ltr;all:revert-layer}
        .invalid{direction:rtl;direction:auto}
    </style><main dir=ltr>
        <p id=ordinary></p><p class=initial></p><p class=inherit></p><p class=unset></p>
        <p class=revert></p><p class=allInitial></p><p class=allInherit></p>
        <p class=allUnset></p><p class=allRevert></p><p class=allLayer></p><p class=invalid></p>
    </main><script>
        const check=(condition,label)=>{if(!condition)throw Error(label);};
        const expected=['rtl','ltr','rtl','rtl','rtl','rtl','ltr','ltr','rtl','ltr','rtl'];
        [...document.querySelectorAll('p')].forEach((node,i)=>check(getComputedStyle(node).direction===expected[i],'direction '+i));
        check(ordinary.matches(':dir(ltr)'),'CSS must not change HTML directionality');
        check(getComputedStyle(ordinary).textAlign==='end','logical alignment stays logical when inherited');
        check(CSS.supports('direction','rtl')&&!CSS.supports('direction','auto'),'native direction grammar');
        ordinary.style.direction='rtl';ordinary.style.direction='auto';
        check(ordinary.style.direction==='rtl','invalid CSSOM assignment preserves prior declaration');
        ordinary.style.direction='ltr';check(getComputedStyle(ordinary).direction==='ltr','inline change invalidates cascade');
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
}

#[test]
fn text_direction_html_hints_css_precedence_shadow_and_auto_mutation() {
    let (_, outcome) = execute_html(
        r#"<main dir=rtl><p id=inherited>English</p><p id=overridden style=direction:ltr>Hebrew</p>
        <p id=automatic dir=auto>English</p><input type=tel value=123>
        <section id=host style=direction:ltr></section><bdi id=isolated>שלום</bdi></main><script>
        const check=(condition,label)=>{if(!condition)throw Error(label);};
        check(getComputedStyle(inherited).direction==='rtl','HTML direction hint inherits');
        check(getComputedStyle(overridden).direction==='ltr'&&overridden.matches(':dir(rtl)'),'CSS overrides style only');
        check(getComputedStyle(automatic).direction==='ltr','auto first strong');
        automatic.textContent='שלום';check(getComputedStyle(automatic).direction==='rtl','auto recomputes after text mutation');
        check(getComputedStyle(document.querySelector('input')).direction==='ltr','telephone hint');
        check(getComputedStyle(isolated).direction==='rtl','bdi auto direction');
        const root=host.attachShadow({mode:'open'});root.innerHTML='<p id=inner>shadow</p>';
        check(getComputedStyle(root.firstChild).direction==='ltr','shadow inherits host computed direction');
        host.style.direction='rtl';check(getComputedStyle(root.firstChild).direction==='rtl','host mutation invalidates inherited direction');
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
}

#[test]
fn canvas_text_direction_inherit_observes_css_without_author_accessors() {
    let (_, outcome) = execute_html(
        r#"<main style=direction:ltr><canvas id=canvas width=200 height=60></canvas></main><script>
        const check=(condition,label)=>{if(!condition)throw Error(label);};
        const ctx=canvas.getContext('2d');ctx.font='20px Arial';ctx.textAlign='start';
        const ltr=ctx.measureText('XX');
        const style=canvas.parentElement.style;style.direction='rtl';
        const rtl=ctx.measureText('XX');
        check(Math.abs(rtl.actualBoundingBoxLeft-ltr.actualBoundingBoxLeft-ltr.width)<0.001,'CSS changes actual aligned ink metrics');
        check(ctx.direction==='inherit','public Canvas state retains inherit');
        const nativeComputed=getComputedStyle;window.getComputedStyle=()=>{throw Error('author computed style');};
        Object.defineProperty(canvas,'dir',{get(){throw Error('author dir');}});
        const still=ctx.measureText('XX');check(still.actualBoundingBoxLeft===rtl.actualBoundingBoxLeft,'private environment reads');
        ctx.direction='ltr';check(ctx.measureText('XX').actualBoundingBoxLeft===ltr.actualBoundingBoxLeft,'explicit Canvas override');
        ctx.direction='inherit';ctx.textAlign='left';const left=ctx.measureText('XX');
        style.direction='ltr';check(ctx.measureText('XX').actualBoundingBoxLeft===left.actualBoundingBoxLeft,'physical left ignores direction');
        window.getComputedStyle=nativeComputed;
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
}
