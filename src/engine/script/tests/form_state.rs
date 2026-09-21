use super::*;

// Red-first Phase A/B coverage: each test asserts specified behavior that the
// baseline does not implement yet. They must fail on the baseline and pass
// after the control-state and validation slices land.

fn check(html: &str) -> (dom::Dom, ScriptOutcome) {
    let (dom, outcome) = execute_html(html);
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    (dom, outcome)
}

#[test]
fn input_default_change_does_not_overwrite_a_dirty_live_value() {
    let (dom, _) = check(
        r#"<body><input id=i value="a"><output></output><script>
            const input = document.getElementById('i');
            const seen = [input.value];
            input.setAttribute('value', 'pristine-follow');
            seen.push(input.value);
            input.value = 'dirty-edit';
            seen.push(input.getAttribute('value'));
            input.setAttribute('value', 'new-default');
            seen.push(input.value, input.getAttribute('value'), input.defaultValue);
            document.querySelector('output').textContent = seen.join('|');
        </script></body>"#,
    );
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "a|pristine-follow|pristine-follow|dirty-edit|new-default|new-default"
    );
}

#[test]
fn textarea_reset_restores_the_current_default() {
    let (dom, _) = check(
        r#"<body><form><textarea name=t>default</textarea></form><output></output><script>
            const area = document.querySelector('textarea');
            area.value = 'edited';
            area.textContent = 'changed-default';
            document.querySelector('form').reset();
            document.querySelector('output').textContent = area.value + '|' + area.defaultValue;
        </script></body>"#,
    );
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "changed-default|changed-default"
    );
}

#[test]
fn select_explicit_no_selection_is_not_index_zero() {
    let (dom, _) = check(
        r#"<body><select><option value=a>A</option><option value=b>B</option></select><output></output><script>
            const select = document.querySelector('select');
            const seen = [select.selectedIndex, select.value];
            select.value = 'missing';
            seen.push(select.selectedIndex, select.value === '');
            select.selectedIndex = -1;
            seen.push(select.selectedIndex, select.value === '');
            document.querySelector('output').textContent = seen.join('|');
        </script></body>"#,
    );
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "0|a|-1|true|-1|true"
    );
}

#[test]
fn option_selected_does_not_rewrite_default_selected() {
    let (dom, _) = check(
        r#"<body><select><option value=a>A</option><option value=b selected>B</option></select><output></output><script>
            const select = document.querySelector('select');
            const [first, second] = select.options;
            first.selected = true;
            document.querySelector('output').textContent = [
                first.selected, second.selected,
                first.hasAttribute('selected'), second.hasAttribute('selected'),
                select.value
            ].join('|');
        </script></body>"#,
    );
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "true|false|false|true|a"
    );
}

#[test]
fn validity_object_is_stable_and_live() {
    let (dom, _) = check(
        r#"<body><input id=i required value="x"><output></output><script>
            const input = document.getElementById('i');
            const same = input.validity === input.validity;
            const saved = input.validity;
            const before = saved.valid;
            input.value = '';
            document.querySelector('output').textContent =
                [same, before, saved.valid, saved.valueMissing].join('|');
        </script></body>"#,
    );
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "true|true|false|true"
    );
}

#[test]
fn step_mismatch_is_detected_for_decimal_steps() {
    let (dom, _) = check(
        r#"<body><input type=number step="0.1"><input type=number step="0.1" min="0.5"><output></output><script>
            const [plain, based] = document.querySelectorAll('input');
            plain.value = '0.3';
            based.value = '0.7';
            const aligned = [plain.validity.stepMismatch, based.validity.stepMismatch].join('|');
            plain.value = '0.35';
            based.value = '0.75';
            document.querySelector('output').textContent = aligned + '|' + [
                plain.validity.stepMismatch, based.validity.stepMismatch,
                based.validity.valid, plain.value
            ].join('|');
        </script></body>"#,
    );
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "false|false|true|true|false|0.35"
    );
}

#[test]
fn single_label_email_domain_is_valid() {
    let (dom, _) = check(
        r#"<body><input type=email value="user@intranet"><output></output><script>
            const input = document.querySelector('input');
            document.querySelector('output').textContent =
                [input.validity.typeMismatch, input.validity.valid].join('|');
        </script></body>"#,
    );
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "false|true"
    );
}

#[test]
fn internal_submit_ignores_an_overridden_report_validity() {
    let (_, outcome) = execute_html(
        r#"<form><input name=q required></form><script>
            const form = document.querySelector('form');
            let submits = 0;
            form.addEventListener('submit', () => submits++);
            form.reportValidity = () => true;
            form.requestSubmit();
            document.title = 'submits=' + submits;
        </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert!(
        outcome.navigation_url.is_none(),
        "{:?}",
        outcome.navigation_url
    );
}

#[test]
fn required_checkbox_and_radio_group_missing() {
    let (dom, _) = check(
        r#"<body><input type=checkbox required><input type=radio name=g required><input type=radio name=g><output></output><script>
            const [box, first] = document.querySelectorAll('input');
            document.querySelector('output').textContent = [
                box.validity.valueMissing, first.validity.valueMissing,
                box.matches(':invalid'), first.matches(':invalid')
            ].join('|');
        </script></body>"#,
    );
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "true|true|true|true"
    );
}

#[test]
fn readonly_bars_every_input_state() {
    let (dom, _) = check(
        r#"<body><input required readonly value=""><input type=checkbox required readonly><output></output><script>
            const [text, box] = document.querySelectorAll('input');
            document.querySelector('output').textContent = [
                text.willValidate, text.validity.valueMissing, text.validity.valid,
                box.willValidate, box.validity.valueMissing
            ].join('|');
        </script></body>"#,
    );
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "false|false|true|false|true"
    );
}

#[test]
fn type_transitions_propagate_and_reload_values() {
    let (dom, _) = check(
        r#"<body><input id=i value="hello"><output></output><script>
            const input = document.getElementById('i');
            const seen = [input.value, input.type];
            input.type = 'invalid-state';
            seen.push(input.type, input.value, input.getAttribute('value'));
            input.value = 'edited';
            input.type = 'checkbox';
            seen.push(input.type, input.value, input.getAttribute('value'));
            input.type = 'text';
            seen.push(input.type, JSON.stringify(input.value), input.getAttribute('value'));
            document.querySelector('output').textContent = seen.join('|');
        </script></body>"#,
    );
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "hello|text|text|hello|hello|checkbox|edited|edited|text|\"edited\"|edited"
    );
}

#[test]
fn clones_copy_live_state_but_not_validation_messages() {
    let (dom, _) = check(
        r#"<body><input value="a"><textarea>default</textarea><output></output><script>
            const input = document.querySelector('input');
            const area = document.querySelector('textarea');
            input.value = 'live';
            area.value = 'edited';
            const inputClone = input.cloneNode();
            const areaClone = area.cloneNode(true);
            input.setAttribute('value', 'new-default');
            document.querySelector('output').textContent = [
                inputClone.value, inputClone.defaultValue,
                areaClone.value, areaClone.defaultValue,
                input.value, inputClone.getAttribute('value')
            ].join('|');
        </script></body>"#,
    );
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "live|a|edited|default|live|a"
    );
}

#[test]
fn textarea_children_track_default_until_dirty() {
    let (dom, _) = check(
        r#"<body><textarea>one</textarea><output></output><script>
            const area = document.querySelector('textarea');
            const seen = [area.value, area.defaultValue];
            area.textContent = 'two';
            seen.push(area.value, area.defaultValue);
            area.value = 'edited';
            area.textContent = 'three';
            seen.push(area.value, area.defaultValue, area.textLength ?? 'n/a');
            document.querySelector('output').textContent = seen.join('|');
        </script></body>"#,
    );
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "one|one|two|two|edited|three|6"
    );
}

#[test]
fn output_value_and_default_modes_reset() {
    let (dom, _) = check(
        r#"<body><form><output name=o>start</output></form><output id=log></output><script>
            const field = document.querySelector('[name=o]');
            const seen = [field.value, field.defaultValue];
            field.value = 'live';
            seen.push(field.value, field.defaultValue, field.textContent);
            field.defaultValue = 'preset';
            seen.push(field.value, field.defaultValue);
            document.querySelector('form').reset();
            seen.push(field.value, field.defaultValue);
            document.getElementById('log').textContent = seen.join('|');
        </script></body>"#,
    );
    assert_eq!(
        dom.elements_named("output").nth(1).unwrap().text_content(),
        "start|start|live|start|live|live|preset|preset|preset"
    );
}

#[test]
fn select_multiple_optgroups_duplicates_and_mutations() {
    let (dom, _) = check(
        r#"<body><select multiple>
              <optgroup label=g><option value=x>X</option><option value=y selected>Y</option></optgroup>
              <option value=x>dup</option>
            </select><output></output><script>
            const select = document.querySelector('select');
            const seen = [select.type, select.length, select.value, select.selectedIndex];
            const saved = select.options;
            // Duplicate values select the first match only.
            select.value = 'x';
            seen.push(select.selectedIndex, select.selectedOptions.length, select.selectedOptions[0].textContent);
            // Dynamic insertion and removal renormalize membership.
            const extra = document.createElement('option');
            extra.value = 'z';
            select.appendChild(extra);
            select.options[0].remove();
            seen.push(saved === select.options, select.options.length, select.options[0].value, JSON.stringify(select.value));
            document.querySelector('output').textContent = seen.join('|');
        </script></body>"#,
    );
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "select-multiple|3|y|1|0|1|X|true|3|y|\"\""
    );
}

#[test]
fn number_programmatic_invalid_empties_without_user_state() {
    let (dom, _) = check(
        r#"<body><input type=number value="12"><input type=range><output></output><script>
            const [number, range] = document.querySelectorAll('input');
            const seen = [number.value];
            number.value = 'abc';
            seen.push(JSON.stringify(number.value), number.getAttribute('value'));
            seen.push(range.value, JSON.stringify(range.min), JSON.stringify(range.max));
            range.value = 'not a number';
            seen.push(range.value);
            document.querySelector('output').textContent = seen.join('|');
        </script></body>"#,
    );
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "12|\"\"|12|50|\"\"|\"\"|50"
    );
}

#[test]
fn hidden_and_button_values_stay_attribute_backed() {
    let (dom, _) = check(
        r#"<body><input type=hidden value="h"><input type=submit value="s"><output></output><script>
            const [hidden, submit] = document.querySelectorAll('input');
            hidden.value = 'changed';
            submit.defaultValue = 'go';
            document.querySelector('output').textContent = [
                hidden.value, hidden.getAttribute('value'), hidden.defaultValue,
                submit.value, submit.getAttribute('value')
            ].join('|');
        </script></body>"#,
    );
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "changed|changed|changed|go|go"
    );
}

#[test]
fn reset_covers_external_controls_and_keeps_order() {
    let (dom, _) = check(
        r#"<body><form id=f></form>
            <input form=f value="a"><textarea form=f>default</textarea>
            <select form=f><option selected>A</option><option>B</option></select>
            <output></output><script>
            const form = document.getElementById('f');
            const [input, area, select] = [document.querySelector('input'),
                document.querySelector('textarea'), document.querySelector('select')];
            input.value = 'edited';
            area.value = 'changed';
            select.selectedIndex = 1;
            let resets = 0;
            form.addEventListener('reset', () => resets++);
            form.reset();
            document.querySelector('output').textContent =
                [resets, input.value, area.value, select.value].join('|');
        </script></body>"#,
    );
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "1|a|default|A"
    );
}

#[test]
fn selected_index_setter_edges_yield_no_selection() {
    let (dom, _) = check(
        r#"<body><select><option value=a>A</option><option value=b>B</option></select><output></output><script>
            const select = document.querySelector('select');
            const seen = [];
            select.selectedIndex = 7;
            seen.push(select.selectedIndex, select.value === '');
            select.selectedIndex = -2;
            seen.push(select.selectedIndex);
            select.selectedIndex = NaN;
            seen.push(select.selectedIndex, select.value);
            document.querySelector('output').textContent = seen.join('|');
        </script></body>"#,
    );
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "-1|true|-1|0|a"
    );
}
