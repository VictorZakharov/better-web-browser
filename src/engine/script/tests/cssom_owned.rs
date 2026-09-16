use super::cssom::{execute_html_with_stylesheets, result};

#[test]
fn imported_rules_enforce_origin_boundaries_and_keep_conditional_metadata() {
    let (dom, outcome) = execute_html_with_stylesheets(
        r#"
    <style id=s>@import 'https://other.test/child.css';</style><body><script>
    const failures=[], check=(n,v)=>{if(!v)failures.push(n)};
    const sheet=s.sheet.cssRules[0].styleSheet;
    check('metadata',sheet.href==='https://other.test/child.css' && sheet.ownerNode===null);
    for(const action of [()=>sheet.cssRules,()=>sheet.insertRule('p{}'),()=>sheet.deleteRule(0)]) {
      try{action();failures.push('origin')}catch(e){check('security',e.name==='SecurityError')}
    }
    s.sheet.insertRule('@import "unloaded.css" layer(theme) supports(display: grid) print;',1);
    const rule=s.sheet.cssRules[1];
    check('conditions',rule.layerName==='theme' && rule.supportsText==='display: grid' &&
      rule.media.mediaText==='print' && rule.styleSheet===null);
    s.sheet.insertRule('p { color: blue }',2);
    try{s.sheet.insertRule('@import "too-late.css";',3);failures.push('late')}
    catch(e){check('hierarchy',e.name==='HierarchyRequestError')}
    try{new CSSStyleSheet().insertRule('/* prefix */ @import "child.css";');failures.push('constructed')}
    catch(e){check('constructed syntax',e.name==='SyntaxError')}
    try{new CSSImportRule(s.sheet,'@import "child.css";',['child.css','',null,null]);failures.push('constructor')}
    catch(e){check('illegal constructor',e.name==='TypeError')}
    document.body.setAttribute('data-result',failures.join(',')||'pass');
    </script>"#,
        vec![("https://other.test/child.css".into(), "p{color:red}".into())],
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(result(&dom).as_deref(), Some("pass"));
}

#[test]
fn imported_cssom_is_live_owned_and_applies_without_mutating_dom_text() {
    let (dom, outcome) = execute_html_with_stylesheets(
        r#"
    <style id=a>@import 'child.css';</style><style id=b>@import 'child.css';</style>
    <p id=target>test</p><script>
    const a=document.getElementById('a'), b=document.getElementById('b'), failures=[];
    const check=(name, yes)=>{if(!yes)failures.push(name)};
    const left=a.sheet.cssRules[0], right=b.sheet.cssRules[0];
    const original=a.textContent, child=left.styleSheet;
    check('identity',child===left.styleSheet && child!==right.styleSheet);
    check('ownership',child.ownerNode===null && child.ownerRule===left &&
      child.parentStyleSheet===a.sheet && left.parentStyleSheet===a.sheet);
    check('initial',getComputedStyle(target).color==='rgb(255, 0, 0)');
    right.styleSheet.cssRules[0].style.color='blue';
    check('cascade',getComputedStyle(target).color==='rgb(0, 0, 255)');
    check('independent',child.cssRules[0].style.color==='red');
    check('text',a.textContent===original);
    right.media='print';
    check('media',getComputedStyle(target).color==='rgb(255, 0, 0)');
    left.styleSheet.disabled=true;
    check('disabled',getComputedStyle(target).color==='rgb(0, 0, 0)');
    child.disabled=false;
    a.sheet.deleteRule(0);
    check('detach',left.parentStyleSheet===null && child.parentStyleSheet===null);
    b.sheet.insertRule('p { color: green; }',1);
    check('insert',getComputedStyle(target).color==='rgb(0, 128, 0)');
    b.sheet.disabled=true;
    check('root disabled',getComputedStyle(target).color==='rgb(0, 0, 0)');
    b.sheet.disabled=false;
    const replaced=b.sheet;
    b.firstChild.data='p {color: purple}';
    replaced.disabled=true;
    check('text reset',getComputedStyle(target).color==='rgb(128, 0, 128)');
    const old=b.sheet;old.cssRules[0].style.color='orange';
    b.remove();check('owner detached',old.ownerNode===null);
    document.head.append(b);
    check('reinsert',b.sheet!==old && getComputedStyle(target).color==='rgb(128, 0, 128)');
    document.body.setAttribute('data-result',failures.join(',')||'pass');
    </script>"#,
        vec![(
            "https://example.com/child.css".into(),
            "p{color:red}".into(),
        )],
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(result(&dom).as_deref(), Some("pass"));
}

#[test]
fn preferred_sets_follow_insertion_order_and_explicit_sheet_toggles() {
    let (dom, outcome) = execute_html_with_stylesheets(
        r#"<p id=target>test</p><script>
    const failures=[], check=(n,v)=>{if(!v)failures.push(n)};
    function add(title,color) { const s=document.createElement('style');s.title=title;
      s.textContent='p{color:'+color+'}';document.head.prepend(s);return s; }
    const first=add('day','green'), second=add('night','red');
    check('preferred',getComputedStyle(target).color==='rgb(0, 128, 0)' && second.sheet.disabled);
    first.sheet.disabled=true;second.sheet.disabled=false;
    check('toggle',getComputedStyle(target).color==='rgb(255, 0, 0)');
    check('no attribute',!first.hasAttribute('disabled'));
    first.remove();
    const last=add('day','blue');
    check('persistent preference',!last.sheet.disabled);
    const host=document.createElement('div');document.body.append(host);
    const shadow=host.attachShadow({mode:'open'});
    shadow.innerHTML='<style title=one>p{color:red}</style><style title=two>p{color:blue}</style><p>shadow</p>';
    check('shadow titles',shadow.styleSheets[0].title===null && !shadow.styleSheets[1].disabled &&
      getComputedStyle(shadow.querySelector('p')).color==='rgb(0, 0, 255)');
    document.body.setAttribute('data-result',failures.join(',')||'pass');
    </script>"#,
        vec![],
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(result(&dom).as_deref(), Some("pass"));
}

#[test]
fn the_first_title_association_wins_before_any_style_query() {
    let (dom, outcome) = execute_html_with_stylesheets(
        r#"<style id=first>p{color:red}</style>
      <style id=second>p{color:green}</style><p id=target>test</p><script>
      second.title='preferred'; first.title='other';
      document.body.setAttribute('data-result',getComputedStyle(target).color==='rgb(0, 128, 0)'?'pass':'fail');
      </script>"#,
        vec![],
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(result(&dom).as_deref(), Some("pass"));
}
