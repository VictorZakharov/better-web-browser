use super::*;

#[test]
fn svg_view_box_returns_stable_live_base_and_distinct_read_only_animated_rects() {
    let (dom, outcome) = execute_html(
        r#"<svg id=s viewBox='1 2 3 4'></svg><output></output><script>
    const s=document.getElementById('s'), value=s.viewBox, base=value.baseVal, animated=value.animVal;
    const fields=rect=>[rect.x,rect.y,rect.width,rect.height].join(',');
    if(value!==s.viewBox || base!==value.baseVal || base===animated || animated!==value.animVal ||
       !(base instanceof SVGRect) || !(value instanceof SVGAnimatedRect) || fields(base)!=='1,2,3,4')throw Error('identity');
    s.setAttribute('viewBox','5,6,7,8');
    if(fields(base)!=='5,6,7,8' || fields(animated)!=='5,6,7,8')throw Error('attribute sync');
    base.width=20;
    if(s.getAttribute('viewBox')!=='5 6 20 8' || animated.width!==20)throw Error('setter sync');
    try{animated.x=9;throw Error('writable animated');}catch(e){if(e.name!=='NoModificationAllowedError')throw e;}
    s.removeAttribute('viewBox');
    if(fields(base)!=='0,0,0,0')throw Error('removed attribute');
    base.x=7;if(s.getAttribute('viewBox')!=='7 0 0 0')throw Error('write absent');
    const getAttribute=Element.prototype.getAttribute;
    s.getAttribute=s.setAttribute=()=>{throw Error('author override');};
    base.width=30;
    if(base.width!==30 || getAttribute.call(s,'viewBox')!=='7 0 30 0')throw Error('IDL attribute ownership');
    document.querySelector('output').textContent='yes';
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "yes"
    );
}

#[test]
fn svg_view_box_number_grammar_matches_interoperable_browser_behavior() {
    let (_, outcome) = execute_html(
        r#"<svg id=s></svg><script>
    const s=document.getElementById('s'), base=s.viewBox.baseVal;
    const cases=[['1-2 3 4',[1,-2,3,4]],['.5,+2e1,3,4',[.5,20,3,4]],
      ['1 2 -3 4',[1,2,-3,4]],['1 2 0 4',[1,2,0,4]],['1 2 3',[0,0,0,0]],
      ['1 2 3 4 5',[0,0,0,0]],['1 2 3 4,',[0,0,0,0]],['1 2 3 1e50',[0,0,0,0]],
      ['1,,2,3,4',[0,0,0,0]],['1.2.3 4 5',[0,0,0,0]],['NaN 2 3 4',[0,0,0,0]]];
    for(const [source,expected]of cases){s.setAttribute('viewBox',source);
      if([base.x,base.y,base.width,base.height].join(',')!==expected.join(','))throw Error(source);}
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
}

#[test]
fn detached_svg_rect_and_idl_errors_do_not_mutate_author_markup() {
    let (_, outcome) = execute_html(
        r#"<svg id=s viewBox='1 2 3 4'></svg><script>
    const s=document.getElementById('s'), free=s.createSVGRect();
    free.width=-4;free.x=2;if(free.width!==-4 || s.viewBox.baseVal.x!==1)throw Error('detached rect');
    for(const value of [NaN,Infinity,-Infinity,1e50]){
      try{s.viewBox.baseVal.x=value;throw Error('invalid accepted');}catch(e){if(e.name!=='TypeError')throw e;}
      if(s.getAttribute('viewBox')!=='1 2 3 4')throw Error('invalid mutation');}
    for(const type of [SVGRect,SVGAnimatedRect]){
      try{new type();throw Error('constructed');}catch(e){if(e.name!=='TypeError')throw e;}}
    try{Object.getOwnPropertyDescriptor(SVGRect.prototype,'x').get.call({});throw Error('bad receiver');}
    catch(e){if(e.name!=='TypeError')throw e;}
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
}
