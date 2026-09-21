use super::*;

fn result(html: &str) -> (dom::Dom, ScriptOutcome) {
    let (dom, outcome) = execute_html(html);
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    (dom, outcome)
}

#[test]
fn canceled_invalid_events_never_make_a_form_valid_or_submit() {
    let (dom, outcome) = result(
        r#"<form action='/should-not-submit'><input required></form>
        <output></output><script>
        const form = document.querySelector('form'), input = document.querySelector('input');
        let invalids = 0, submits = 0;
        input.addEventListener('invalid', e => { invalids++; e.preventDefault(); });
        form.addEventListener('submit', () => submits++);
        const checked = form.checkValidity(), reported = form.reportValidity();
        form.requestSubmit();
        document.querySelector('output').textContent =
            [checked, reported, invalids, submits, document.activeElement === input].join('|');
        </script>"#,
    );
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "false|false|3|0|false"
    );
    assert!(
        outcome.navigation_url.is_none(),
        "{:?}",
        outcome.navigation_url
    );
}

#[test]
fn invalid_candidates_are_snapshotted_before_dispatching_trusted_events() {
    let (dom, _) = result(
        r#"<form><input id=a required><input id=b required></form>
        <output></output><script>
        const form = document.querySelector('form');
        const a = document.getElementById('a'), b = document.getElementById('b');
        const events = [];
        a.addEventListener('invalid', e => { events.push('a:' + e.isTrusted); b.value = 'corrected'; });
        b.addEventListener('invalid', e => events.push('b:' + e.isTrusted));
        events.push(String(form.checkValidity()));
        document.querySelector('output').textContent = events.join('|');
        </script>"#,
    );
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "a:true|b:true|false"
    );
}

#[test]
fn pattern_validation_ignores_author_regex_overrides() {
    let (dom, _) = result(
        r#"<input pattern='[a-z]+'><output></output><script>
        const input = document.querySelector('input');
        let calls = 0;
        RegExp.prototype.test = function() { calls++; return true; };
        RegExp.prototype.exec = function() { calls++; return ['fake']; };
        input.value = '123';
        const mismatch = input.validity.patternMismatch;
        document.querySelector('output').textContent = [mismatch, input.matches(':invalid'), calls].join('|');
        </script>"#,
    );
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "true|true|0"
    );
}

#[test]
fn range_sanitization_applies_after_defaults_bounds_and_reset() {
    let (dom, _) = result(
        r#"<form><input type=range min=0 max=10 step=3></form>
        <output></output><script>
        const input = document.querySelector('input'), values = [input.value];
        input.value = '5'; values.push(input.value, input.validity.stepMismatch);
        input.max = '4'; values.push(input.value);
        input.defaultValue = '8'; values.push(input.value);
        document.querySelector('form').reset(); values.push(input.value);
        input.removeAttribute('value'); document.querySelector('form').reset(); values.push(input.value);
        input.type = 'text'; input.value = '9'; input.type = 'range'; values.push(input.value);
        document.querySelector('output').textContent = values.join('|');
        </script>"#,
    );
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "6|6|false|3|3|3|3|3"
    );
}

#[test]
fn default_value_modes_follow_attributes_after_type_changes() {
    let (dom, _) = result(
        r#"<input value=initial><output></output><script>
        const input = document.querySelector('input'), values = [];
        input.value = 'edited'; input.type = 'submit';
        values.push(input.value, input.getAttribute('value'));
        input.value = 'send'; values.push(input.value, input.getAttribute('value'));
        input.type = 'hidden'; input.value = 'secret'; values.push(input.value);
        input.type = 'text'; values.push(input.value);
        document.querySelector('output').textContent = values.join('|');
        </script>"#,
    );
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "edited|edited|send|send|secret|secret"
    );
}

#[test]
fn numeric_edit_does_not_submit_the_previous_valid_value() {
    let dom = dom::parse("<input type=number value=12 required>");
    let input = dom.elements_named("input").next().unwrap();
    input.user_edit_input("12e");
    assert_eq!(input.input_value(), "");
    assert_eq!(
        input.control_state_snapshot().editing.as_deref(),
        Some("12e")
    );
    let flags = dom::node::control_validity::validity_of(
        &input,
        &dom::node::control_validity::PatternSource::Cached,
    );
    assert!(flags.bad_input && flags.value_missing);
    input.user_edit_input("125");
    assert_eq!(input.input_value(), "125");
}

#[test]
fn extreme_numeric_exponents_and_temporal_years_do_not_panic() {
    use dom::node::{control_numeric, control_temporal};
    assert!(control_numeric::parse_decimal("0.1e-2147483648").is_some());
    let base = control_numeric::step_base(&None, &None);
    let step = control_numeric::allowed_step("number", &Some("1e-2147483648".into())).unwrap();
    assert!(!control_numeric::step_mismatch("0e-2147483648", base, step));
    for state in ["date", "month", "week", "datetime-local"] {
        let value = match state {
            "date" => "9223372036854775807-01-01",
            "month" => "9223372036854775807-01",
            "week" => "9223372036854775807-W01",
            _ => "9223372036854775807-01-01T00:00",
        };
        let _ = control_temporal::step_rank(state, value);
        let _ = control_temporal::parse_temporal(state, value);
    }
}

#[test]
fn numeric_value_grammar_rejects_incomplete_literals() {
    let dom = dom::parse("<input type=number>");
    let input = dom.elements_named("input").next().unwrap();
    for value in ["+1", "1.", "1.e2", " 1", "1 "] {
        input.set_input_value(value);
        assert_eq!(input.input_value(), "", "{value}");
    }
    for value in [".5", "-1", "1.2e+2"] {
        input.set_input_value(value);
        assert_eq!(input.input_value(), value);
    }
}

#[test]
fn saved_control_collections_stay_live_without_reading_the_getter_again() {
    let (dom, _) = result(
        r#"<form><fieldset><select><option>A</option></select></fieldset></form>
        <output></output><script>
        const form = document.querySelector('form'), fieldset = document.querySelector('fieldset');
        const select = document.querySelector('select'), options = select.options;
        const selected = select.selectedOptions, controls = form.elements, fields = fieldset.elements;
        const added = document.createElement('option'); added.textContent = 'B'; select.append(added);
        select.selectedIndex = 1;
        const input = document.createElement('input'); input.name = 'extra'; fieldset.append(input);
        const values = [options.length, options.item(1).text, selected.length, selected[0].text,
            controls.length, controls.namedItem('extra') === input, fields.length, added.index];
        form.remove();
        values.push(controls.length, fields.length);
        added.remove(); values.push(options.length);
        document.querySelector('output').textContent = values.join('|');
        </script>"#,
    );
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "2|B|1|B|3|true|2|1|3|2|1"
    );
}
