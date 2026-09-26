use super::cssom::{execute_html_with_stylesheets, result};

#[test]
fn layer_rules_expose_names_parentage_and_live_mutation() {
    let (dom, outcome) = execute_html_with_stylesheets(
        r#"<style id=s>@layer base, theme;
          @layer base { p { color: red; } }
          @layer theme { p { color: blue; } }</style>
          <p id=target>test</p><script>
          const failures=[], check=(name, yes)=>{if(!yes)failures.push(name)};
          const sheet=s.sheet, rules=sheet.cssRules;
          check('statement',rules[0] instanceof CSSLayerStatementRule &&
            rules[0].type===0 && CSSRule.LAYER_STATEMENT_RULE===undefined &&
            rules[0].nameList.join(',')==='base,theme' && Object.isFrozen(rules[0].nameList));
          check('blocks',rules[1] instanceof CSSLayerBlockRule &&
            rules[1] instanceof CSSGroupingRule && rules[2].type===0 &&
            rules[1].name==='base' && rules[2].name==='theme');
          check('parent',rules[2].cssRules[0].parentRule===rules[2] &&
            rules[2].cssRules[0].parentStyleSheet===sheet && rules[2].cssRules.item(1)===null);
          check('initial',getComputedStyle(target).color==='rgb(0, 0, 255)');
          rules[2].cssRules[0].style.color='green';
          check('nested mutation',getComputedStyle(target).color==='rgb(0, 128, 0)');
          rules[2].deleteRule(0);
          check('delete',getComputedStyle(target).color==='rgb(255, 0, 0)' &&
            rules[2].cssRules.length===0);
          rules[2].insertRule('p { color: purple; }',0);
          check('insert',getComputedStyle(target).color==='rgb(128, 0, 128)' &&
            rules[2].cssRules[0].parentRule===rules[2]);
          const detached=rules[2], inner=detached.cssRules[0];
          sheet.deleteRule(2);
          check('detach',detached.parentStyleSheet===null && inner.parentStyleSheet===null &&
            inner.parentRule===detached && getComputedStyle(target).color==='rgb(255, 0, 0)');
          document.body.setAttribute('data-result',failures.join(',')||'pass');
          </script>"#,
        vec![],
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(result(&dom).as_deref(), Some("pass"));
}

#[test]
fn constructed_layers_support_nested_rules_and_preserve_import_order() {
    let (dom, outcome) = execute_html_with_stylesheets(
        r#"<p id=target>test</p><script>
          const failures=[], check=(name, yes)=>{if(!yes)failures.push(name)};
          const sheet=new CSSStyleSheet();
          sheet.replaceSync('@layer first, second; @layer first { p { color: red; } }');
          document.adoptedStyleSheets=[sheet];
          check('adopted',getComputedStyle(target).color==='rgb(255, 0, 0)');
          sheet.insertRule('@layer second { @layer inner { p { color: blue; } } }',2);
          check('nested',sheet.cssRules[2].cssRules[0].name==='inner' &&
            sheet.cssRules[2].cssRules[0].parentRule===sheet.cssRules[2] &&
            getComputedStyle(target).color==='rgb(0, 0, 255)');
          sheet.cssRules[2].cssRules[0].insertRule('p { color: green; }',1);
          check('nested insert',getComputedStyle(target).color==='rgb(0, 128, 0)');
          for(const source of ['@layer initial {}','@layer first. second {}','@layer first,;']) {
            try { sheet.insertRule(source); failures.push('invalid '+source); }
            catch(e) { check('syntax '+source,e.name==='SyntaxError'); }
          }
          try { sheet.cssRules[2].insertRule('@import "x.css";'); failures.push('group import'); }
          catch(e) { check('group hierarchy',e.name==='HierarchyRequestError'); }
          const owned=document.createElement('style'); document.head.append(owned);
          owned.sheet.insertRule('@layer base;',0);
          owned.sheet.insertRule('@import "child.css" layer(base);',1);
          owned.sheet.insertRule('@layer after;',2);
          owned.sheet.insertRule('@import "late.css";',3);
          check('layer statement between imports',owned.sheet.cssRules.length===4);
          owned.sheet.insertRule('p { color: red; }',4);
          try { owned.sheet.insertRule('@import "too-late.css";',5); failures.push('late import'); }
          catch(e) { check('late hierarchy',e.name==='HierarchyRequestError'); }
          document.body.setAttribute('data-result',failures.join(',')||'pass');
          </script>"#,
        vec![],
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(result(&dom).as_deref(), Some("pass"));
}

#[test]
fn conditional_groups_expose_nested_layers_and_recompute_after_edits() {
    let (dom, outcome) = execute_html_with_stylesheets(
        r#"<style id=s>@media screen { @layer ui { p {color:red} } }
          @supports (display: flex) { @layer ui { p {color:blue} } }</style>
          <p id=target>test</p><script>
          const failures=[], check=(name, yes)=>{if(!yes)failures.push(name)};
          const media=s.sheet.cssRules[0], supports=s.sheet.cssRules[1];
          check('interfaces',media instanceof CSSMediaRule && media instanceof CSSConditionRule &&
            supports instanceof CSSSupportsRule && supports.type===CSSRule.SUPPORTS_RULE);
          check('nesting',media.cssRules[0] instanceof CSSLayerBlockRule &&
            media.cssRules[0].parentRule===media &&
            supports.cssRules[0].cssRules[0].parentRule===supports.cssRules[0]);
          check('initial',getComputedStyle(target).color==='rgb(0, 0, 255)');
          check('conditions',media.matches && supports.matches &&
            media.conditionText===media.media.mediaText);
          s.sheet.deleteRule(1);
          check('supports delete',getComputedStyle(target).color==='rgb(255, 0, 0)' &&
            supports.parentStyleSheet===null);
          media.media.mediaText='print';
          check('media edit',!media.matches && getComputedStyle(target).color==='rgb(0, 0, 0)');
          media.media.mediaText='screen';
          media.cssRules[0].cssRules[0].style.color='green';
          check('nested edit',getComputedStyle(target).color==='rgb(0, 128, 0)');
          document.body.setAttribute('data-result',failures.join(',')||'pass');
          </script>"#,
        vec![],
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(result(&dom).as_deref(), Some("pass"));
}

#[test]
fn conditional_cssom_uses_media_list_parsing_and_overload_specific_supports() {
    let (dom, outcome) = execute_html_with_stylesheets(
        r#"<style id=owned>@media screen { p { color: red } }</style>
          <p>test</p><script>
          const failures=[], check=(name, yes)=>{if(!yes)failures.push(name)};
          const owned=document.getElementById('owned');
          const media=owned.sheet.cssRules[0];
          check('owned media matches',media.matches);
          owned.remove();
          check('detached media does not match',!media.matches);
          document.head.append(owned);
          check('reattached media matches',owned.sheet.cssRules[0].matches);

          const sheet=new CSSStyleSheet({media:'screen, print'});
          const list=sheet.media;
          check('indexed getters',list.length===2 && list[0]==='screen' &&
            list[1]==='print' && list[2]===undefined);
          list.deleteMedium('SCREEN');
          check('canonical delete',list.length===1 && list[0]==='print' &&
            list[1]===undefined);
          list.appendMedium('screen');
          list.appendMedium('SCREEN');
          check('canonical append',list.length===2 && list[1]==='screen');
          list.appendMedium('screen, print');
          check('list is not a single medium',list.length===2);
          list.mediaText=null;
          check('null empties the list',list.length===0 && list.mediaText==='' &&
            list[0]===undefined);
          let missing=false;
          try { list.deleteMedium('screen'); }
          catch(error) { missing=error.name==='NotFoundError'; }
          check('missing medium throws',missing);

          check('one argument implicit declaration',CSS.supports('display: grid'));
          check('one argument importance',CSS.supports('display: grid !important'));
          check('two argument declaration',CSS.supports('display','grid'));
          check('two argument importance rejected',!CSS.supports('display','grid !important'));
          check('two argument selector rejected',!CSS.supports('selector(div > .x)',''));
          const syntax=new CSSStyleSheet();
          syntax.replaceSync('@supports ({}) { p { color: red; } } ' +
            '@supports ({x}) { p { color: blue; } }');
          check('component braces do not split rules',syntax.cssRules.length===2 &&
            syntax.cssRules[0].conditionText==='({})' &&
            syntax.cssRules[1].conditionText==='({x})' &&
            syntax.cssRules[0].cssRules.length===1);
          const invalid=new CSSStyleSheet();
          invalid.replaceSync('@supports not (display: grid) trailing { p { color: red; } }');
          check('invalid supports ignored',invalid.cssRules.length===0);
          let rejected=false;
          try { invalid.insertRule('@supports not (display: grid) trailing {}'); }
          catch(error) { rejected=error.name==='SyntaxError'; }
          check('invalid supports insertion throws',rejected);
          document.body.setAttribute('data-result',failures.join(',')||'pass');
          </script>"#,
        vec![],
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(result(&dom).as_deref(), Some("pass"));
}

#[test]
fn scope_rules_reflect_boundaries_and_keep_grouping_identity() {
    let (dom, outcome) = execute_html_with_stylesheets(
        r#"<style id=s>@scope (.card) to (.stop) {
              .label { color: red; }
            }</style><body><div class=card><p class=label>test</p></div><script>
          const failures=[], check=(name, yes)=>{if(!yes)failures.push(name)};
          const sheet=document.getElementById('s').sheet;
          const scope=sheet.cssRules[0];
          check('interface',scope instanceof CSSScopeRule &&
            scope instanceof CSSGroupingRule && scope.type===0);
          check('boundaries',scope.start==='.card' && scope.end==='.stop');
          check('children',scope.cssRules.length===1 &&
            scope.cssRules[0] instanceof CSSStyleRule &&
            scope.cssRules[0].parentRule===scope &&
            scope.cssRules[0].parentStyleSheet===sheet);
          check('original serialization',scope.cssText.includes('@scope (.card) to (.stop)'));
          scope.insertRule('.other { color: green; }',1);
          check('mutation serialization',scope.cssText.includes('@scope (.card) to (.stop)') &&
            scope.cssText.includes('.other') && scope.cssRules.length===2);
          const anonymous=new CSSStyleSheet();
          anonymous.replaceSync('@scope { p { color: blue; } }');
          check('implicit boundaries',anonymous.cssRules[0] instanceof CSSScopeRule &&
            anonymous.cssRules[0].start===null && anonymous.cssRules[0].end===null);
          anonymous.insertRule('@scope (.a,.b) to (.x,.y) { p { color: green; } }',1);
          check('boundary serialization',anonymous.cssRules[1].start==='.a, .b' &&
            anonymous.cssRules[1].end==='.x, .y');
          let rejected=false;
          try { anonymous.insertRule('@scope (.card >) { p { color: red; } }'); }
          catch(error) { rejected=error.name==='SyntaxError'; }
          check('invalid boundary rejected',rejected);
          document.body.setAttribute('data-result',failures.join(',')||'pass');
          </script>"#,
        vec![],
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(result(&dom).as_deref(), Some("pass"));
}

#[test]
fn scoped_imports_preserve_cssom_identity_and_serialize_the_scope_modifier() {
    let (dom, outcome) = execute_html_with_stylesheets(
        r#"<style id=s>@import "child.css" scope(.card) layer(ui) supports(display: grid) screen;
          @import "child.css" scope;</style>
          <div class=card><p id=inside>inside</p></div><p id=outside>outside</p>
          <script>
          const failures=[], check=(name, yes)=>{if(!yes)failures.push(name)};
          const sheet=s.sheet, first=sheet.cssRules[0], second=sheet.cssRules[1];
          check('import instances',first instanceof CSSImportRule &&
            second instanceof CSSImportRule && first!==second);
          check('scope serialization',first.cssText.includes('scope(.card)') &&
            first.cssText.includes('layer(ui)') &&
            first.cssText.includes('supports(display: grid)') &&
            second.cssText.includes(' scope;'));
          check('existing import metadata',first.layerName==='ui' &&
            first.supportsText==='display: grid' && first.media.mediaText==='screen');
          check('per occurrence sheets',first.styleSheet!==null &&
            second.styleSheet!==null && first.styleSheet!==second.styleSheet &&
            first.styleSheet.ownerRule===first && second.styleSheet.ownerRule===second);
          let invalid=false;
          try { sheet.insertRule('@import "child.css" scope(.card::before);',0); }
          catch(error) { invalid=error.name==='SyntaxError'; }
          check('invalid scope import rejects insertion',invalid);
          document.body.setAttribute('data-result',failures.join(',')||'pass');
          </script>"#,
        vec![(
            "https://example.com/child.css".into(),
            "p { color: red }".into(),
        )],
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(result(&dom).as_deref(), Some("pass"));
}

#[test]
fn scope_declaration_runs_are_nested_declaration_rules_in_source_order() {
    let (dom, outcome) = execute_html_with_stylesheets(
        r#"<style id=s>@scope (.card) {
              color: red;
              p { color: blue; }
              background-color: green;
            }</style><div class=card><p>test</p></div><script>
            const scope=s.sheet.cssRules[0], rules=scope.cssRules;
            const card=document.querySelector('.card');
            const pass=rules.length===3 &&
              rules[0] instanceof CSSNestedDeclarations &&
              rules[0].style.color==='red' &&
              rules[1] instanceof CSSStyleRule &&
              rules[2] instanceof CSSNestedDeclarations &&
              rules[2].style.backgroundColor==='green' &&
              rules[0].parentRule===scope && rules[2].parentStyleSheet===s.sheet &&
              getComputedStyle(card).color==='rgb(255, 0, 0)' &&
              getComputedStyle(card).backgroundColor==='rgb(0, 128, 0)' &&
              getComputedStyle(card.querySelector('p')).color==='rgb(0, 0, 255)';
            document.body.setAttribute('data-result',pass?'pass':'fail');
            </script>"#,
        vec![],
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(result(&dom).as_deref(), Some("pass"));
}

#[test]
fn media_and_scope_rules_serialize_canonical_group_blocks() {
    let (dom, outcome) = execute_html_with_stylesheets(
        r#"<style id=s>@media screEN {}
            @scope {}
            @scope (.card) to (.stop) { p { color: red; } }</style>
            <script>
            const rules=s.sheet.cssRules;
            const pass=rules.length===3 &&
              rules[0].cssText==='@media screen {\n}' &&
              rules[1].cssText==='@scope {\n}' &&
              rules[2].cssText==='@scope (.card) to (.stop) {\n  p { color: red; }\n}';
            document.body.setAttribute('data-result',pass?'pass':'fail');
            </script>"#,
        vec![],
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(result(&dom).as_deref(), Some("pass"));
}

#[test]
fn import_rule_serializes_control_characters_as_css_escapes() {
    let (dom, outcome) = execute_html_with_stylesheets(
        r#"<style id=s>@import "a\A b.css";</style><script>
          const rule=s.sheet.cssRules[0];
          const pass=rule instanceof CSSImportRule &&
            rule.href.includes('\n') && rule.cssText.includes('\\a ') &&
            !rule.cssText.includes('\n');
          document.body.setAttribute('data-result',pass?'pass':'fail');
          </script>"#,
        vec![],
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(result(&dom).as_deref(), Some("pass"));
}

#[test]
fn conditional_at_rules_do_not_require_whitespace_before_a_parenthesis() {
    let (dom, outcome) = execute_html_with_stylesheets(
        r#"<style id=s>@scope(.card){p{color:red}}
            @media(color){p{color:blue}}
            @supports(display:grid){p{color:green}}</style><script>
            const rules=s.sheet.cssRules;
            const pass=rules.length===3 &&
              rules[0] instanceof CSSScopeRule && rules[0].start==='.card' &&
              rules[1] instanceof CSSMediaRule && rules[1].media.mediaText==='(color)' &&
              rules[2] instanceof CSSSupportsRule && rules[2].matches;
            document.body.setAttribute('data-result',pass?'pass':'fail');
            </script>"#,
        vec![],
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(result(&dom).as_deref(), Some("pass"));
}
