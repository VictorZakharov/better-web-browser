use super::cssom::{execute_html_with_stylesheets, result};
use super::*;

#[test]
fn programmatic_focus_updates_dynamic_selectors_and_computed_styles() {
    let (dom, outcome) = execute_html_with_stylesheets(
        r#"<style>
            section { color: black; }
            section:focus-within { color: blue; }
            input:focus { color: red; }
        </style><section id=panel><input id=field></section><script>
            const color = node => getComputedStyle(node).color;
            const before = color(panel) === 'rgb(0, 0, 0)' && !field.matches(':focus');
            field.focus();
            const during = field.matches(':focus') && panel.matches(':focus-within') &&
                color(field) === 'rgb(255, 0, 0)' && color(panel) === 'rgb(0, 0, 255)';
            field.blur();
            const after = !field.matches(':focus') && !panel.matches(':focus-within') &&
                color(panel) === 'rgb(0, 0, 0)';
            document.body.setAttribute('data-result', [before, during, after].join(','));
        </script>"#,
        vec![],
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(result(&dom).as_deref(), Some("true,true,true"));
}

#[test]
fn programmatic_focus_requests_a_render_for_ancestor_selectors() {
    let (dom, outcome) = execute_html_with_stylesheets(
        "<section><input id=field></section><script>field.focus()</script>",
        vec![],
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert!(outcome.render_requested);
    assert_eq!(outcome.invalidation.roots, vec![dom.document.id()]);
    assert!(outcome.invalidation.impact.affects_style());
}

#[test]
fn native_focus_and_blur_update_selector_state() {
    let dom = dom::parse("<section><input id=field></section>");
    let section = dom.elements_named("section").next().unwrap();
    let field = dom.elements_named("input").next().unwrap();
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    assert!(runtime.execute_initial(&[]).errors.is_empty());
    let outcome = runtime.dispatch_user_input(UserInputEvent::Focus {
        target: Some(field.clone()),
        focused: true,
    });
    assert!(
        outcome.outcome.errors.is_empty(),
        "{:?}",
        outcome.outcome.errors
    );
    assert!(field.is_focused());
    assert!(section.has_focus_within());
    let outcome = runtime.dispatch_user_input(UserInputEvent::Focus {
        target: None,
        focused: false,
    });
    assert!(
        outcome.outcome.errors.is_empty(),
        "{:?}",
        outcome.outcome.errors
    );
    assert!(!field.is_focused());
    assert!(!section.has_focus_within());
}

#[test]
fn language_and_direction_attribute_changes_restyle_descendants() {
    let (dom, outcome) = execute_html_with_stylesheets(
        r#"<style>
            span { color: black; background-color: white; }
            span:lang(fr) { color: red; }
            span:dir(rtl) { background-color: blue; }
        </style><section id=panel lang=en dir=ltr><span id=label>text</span></section><script>
            const before = getComputedStyle(label).color === 'rgb(0, 0, 0)' &&
                getComputedStyle(label).backgroundColor === 'rgb(255, 255, 255)';
            panel.setAttribute('lang', 'fr-CA');
            panel.setAttribute('dir', 'rtl');
            const after = label.matches(':lang(fr)') && label.matches(':dir(rtl)') &&
                getComputedStyle(label).color === 'rgb(255, 0, 0)' &&
                getComputedStyle(label).backgroundColor === 'rgb(0, 0, 255)';
            document.body.setAttribute('data-result', [before, after].join(','));
        </script>"#,
        vec![],
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(result(&dom).as_deref(), Some("true,true"));
}
