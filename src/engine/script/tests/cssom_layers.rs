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
          try { owned.sheet.insertRule('@import "late.css";',3); failures.push('late import'); }
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
          supports.conditionText='(display: unsupported-value)';
          check('supports edit',getComputedStyle(target).color==='rgb(255, 0, 0)');
          media.media.mediaText='print';
          check('media edit',getComputedStyle(target).color==='rgb(0, 0, 0)');
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
