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
