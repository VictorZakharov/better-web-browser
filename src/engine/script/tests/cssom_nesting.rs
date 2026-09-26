use super::cssom::{execute_html_with_stylesheets, result};

#[test]
fn nested_style_rules_expose_ordered_child_rules_and_live_declarations() {
    let (dom, outcome) = execute_html_with_stylesheets(
        r#"<style id=s>.card { color: red; & { color: blue; } color: green; }</style>
          <div id=target class=card>test</div><script>
          const failures=[], check=(name, yes)=>{if(!yes)failures.push(name)};
          const rule=s.sheet.cssRules[0];
          check('group inheritance',rule instanceof CSSStyleRule &&
            rule instanceof CSSGroupingRule && rule.cssRules.length===2);
          check('declarations',rule.style.color==='red' &&
            rule.cssRules[1] instanceof CSSNestedDeclarations &&
            rule.cssRules[1].type===0 && rule.cssRules[1].style.color==='green');
          check('parentage',rule.cssRules[0].parentRule===rule &&
            rule.cssRules[1].parentStyleSheet===s.sheet);
          check('initial',getComputedStyle(target).color==='rgb(0, 128, 0)');
          rule.cssRules[1].style.color='purple';
          check('nested declaration mutation',getComputedStyle(target).color==='rgb(128, 0, 128)');
          rule.insertRule('& { color: orange; }',2);
          check('insert',rule.cssRules.length===3 &&
            getComputedStyle(target).color==='rgb(255, 165, 0)');
          rule.deleteRule(2);
          check('delete',getComputedStyle(target).color==='rgb(128, 0, 128)');
          document.body.setAttribute('data-result',failures.join(',')||'pass');
          </script>"#,
        vec![],
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(result(&dom).as_deref(), Some("pass"));
}

#[test]
fn nested_condition_groups_expose_direct_declarations() {
    let (dom, outcome) = execute_html_with_stylesheets(
        r#"<style id=s>.card { @media (min-width: 1px) { color: blue; } }</style>
          <div id=target class=card>test</div><script>
          const failures=[], check=(name, yes)=>{if(!yes)failures.push(name)};
          const parent=s.sheet.cssRules[0], media=parent.cssRules[0];
          check('group',media instanceof CSSMediaRule &&
            media.cssRules[0] instanceof CSSNestedDeclarations &&
            media.cssRules[0].parentRule===media);
          check('initial',getComputedStyle(target).color==='rgb(0, 0, 255)');
          media.cssRules[0].style.color='red';
          check('mutation',getComputedStyle(target).color==='rgb(255, 0, 0)');
          document.body.setAttribute('data-result',failures.join(',')||'pass');
          </script>"#,
        vec![],
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(result(&dom).as_deref(), Some("pass"));
}

#[test]
fn nested_insert_rule_accepts_declaration_blocks_and_keeps_them_live() {
    let (dom, outcome) = execute_html_with_stylesheets(
        r#"<style id=s>.card { color: red; & { color: green; } }</style>
          <div id=target class=card>test</div><script>
          const failures=[], check=(name, yes)=>{if(!yes)failures.push(name)};
          const rule=s.sheet.cssRules[0];
          check('before',getComputedStyle(target).color==='rgb(0, 128, 0)');
          check('insert index',rule.insertRule('color: blue; background-color: yellow;',1)===1);
          const declarations=rule.cssRules[1];
          check('rule identity',declarations instanceof CSSNestedDeclarations &&
            declarations.parentRule===rule && declarations.parentStyleSheet===s.sheet &&
            declarations.style.color==='blue' &&
            declarations.style.backgroundColor==='yellow');
          check('cascade order',getComputedStyle(target).color==='rgb(0, 0, 255)' &&
            getComputedStyle(target).backgroundColor==='rgb(255, 255, 0)');
          declarations.style.color='purple';
          check('mutation',getComputedStyle(target).color==='rgb(128, 0, 128)');
          check('serialization',rule.cssText.includes('color: purple;') &&
            rule.cssText.includes('background-color: yellow;'));
          rule.deleteRule(1);
          check('delete',declarations.parentRule===null &&
            getComputedStyle(target).color==='rgb(0, 128, 0)' &&
            getComputedStyle(target).backgroundColor==='rgba(0, 0, 0, 0)');
          document.body.setAttribute('data-result',failures.join(',')||'pass');
          </script>"#,
        vec![],
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(result(&dom).as_deref(), Some("pass"));
}

#[test]
fn scope_and_nested_conditions_insert_direct_declarations_only_where_allowed() {
    let (dom, outcome) = execute_html_with_stylesheets(
        r#"<style id=s>
            @scope (.card) { .title { color: red; } }
            @media (min-width: 1px) { .title { background-color: red; } }
            .card { @supports (display: block) { .title { background-color: red; } } }
          </style>
          <div id=root class=card><span id=target class=title>test</span></div><script>
          const failures=[], check=(name, yes)=>{if(!yes)failures.push(name)};
          const scope=s.sheet.cssRules[0];
          const media=s.sheet.cssRules[1];
          const nestedSupports=s.sheet.cssRules[2].cssRules[0];
          scope.insertRule('color: blue;',1);
          check('scope declaration',scope.cssRules[1] instanceof CSSNestedDeclarations &&
            scope.cssRules[1].parentRule===scope &&
            getComputedStyle(root).color==='rgb(0, 0, 255)' &&
            getComputedStyle(target).color==='rgb(255, 0, 0)');
          nestedSupports.insertRule('background-color: green;',1);
          check('nested condition declaration',
            nestedSupports.cssRules[1] instanceof CSSNestedDeclarations &&
            nestedSupports.cssRules[1].parentRule===nestedSupports &&
            getComputedStyle(root).backgroundColor==='rgb(0, 128, 0)' &&
            getComputedStyle(target).backgroundColor==='rgb(255, 0, 0)');
          let normalGroupRejected=false;
          try { media.insertRule('color: purple;',1); }
          catch (error) { normalGroupRejected=error.name==='SyntaxError'; }
          check('normal grouping rejects declarations',normalGroupRejected &&
            media.cssRules.length===1);
          let topLevelRejected=false;
          try { s.sheet.insertRule('color: purple;',3); }
          catch (error) { topLevelRejected=error.name==='SyntaxError'; }
          check('top-level insert rejects declarations',topLevelRejected &&
            s.sheet.cssRules.length===3);
          const constructed=new CSSStyleSheet();
          constructed.replaceSync('color: purple; .valid { color: green; }');
          check('top-level parse drops declarations',constructed.cssRules.length===1 &&
            constructed.cssRules[0].selectorText==='.valid');
          let emptyRejected=false;
          try { scope.insertRule('not a declaration',2); }
          catch (error) { emptyRejected=error.name==='SyntaxError'; }
          check('invalid declaration rejected',emptyRejected && scope.cssRules.length===2);
          document.body.setAttribute('data-result',failures.join(',')||'pass');
          </script>"#,
        vec![],
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(result(&dom).as_deref(), Some("pass"));
}

#[test]
fn nested_structural_selector_recomputes_after_dom_insertion() {
    let (dom, outcome) = execute_html_with_stylesheets(
        r#"<style>
            @layer base {
                ul {
                    > li:nth-child(2 of .hit) { color: red; }
                }
            }
        </style>
        <ul id=list>
            <li id=first class=hit>first</li>
            <li id=middle>middle</li>
            <li id=last class=hit>last</li>
        </ul>
        <script>
            const red = node => getComputedStyle(node).color === 'rgb(255, 0, 0)';
            const initial = !red(first) && !red(middle) && red(last);
            const added = document.createElement('li');
            added.className = 'hit';
            list.insertBefore(added, middle);
            const inserted = !red(first) && red(added) && !red(last);
            list.removeChild(added);
            const restored = !red(first) && red(last);
            document.body.setAttribute(
                'data-result', [initial, inserted, restored].join(',')
            );
        </script>"#,
        vec![],
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(result(&dom).as_deref(), Some("true,true,true"));
}
