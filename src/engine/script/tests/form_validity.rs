use super::*;
use crate::engine::dom;

// Phase B: live ValidityState, per-type applicability, scalar constraints,
// numeric APIs, custom validity, and check/report event semantics.

fn check(html: &str) -> (dom::Dom, ScriptOutcome) {
    let (dom, outcome) = execute_html(html);
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    (dom, outcome)
}

#[test]
fn validity_state_has_web_idl_shape_and_brand_checks() {
    let (dom, _) = check(
        r#"<body><input required value="x"><output></output><script>
            const input = document.querySelector('input');
            const seen = [];
            try { new ValidityState(); seen.push('constructed'); }
            catch (error) { seen.push(error instanceof TypeError); }
            const descriptor = Object.getOwnPropertyDescriptor(ValidityState.prototype, 'valid');
            seen.push(descriptor.enumerable, descriptor.configurable, typeof descriptor.get);
            seen.push(Object.keys(input.validity).length);
            try { ValidityState.prototype.valueMissing; seen.push('no-throw'); }
            catch (error) { seen.push(error instanceof TypeError); }
            try { validityFlags.call({}); seen.push('no-throw'); }
            catch (error) { seen.push(error instanceof ReferenceError || error instanceof TypeError); }
            seen.push(input.validity instanceof ValidityState);
            document.querySelector('output').textContent = seen.join('|');
        </script></body>"#,
    );
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "true|true|true|function|0|true|true|true"
    );
}

#[test]
fn simultaneous_failures_stay_visible_with_custom_error() {
    let (dom, _) = check(
        r#"<body><input type=email required pattern="[a-z]+"><output></output><script>
            const input = document.querySelector('input');
            input.setCustomValidity('Custom problem.');
            const flags = input.validity;
            // Flag reads run before the message binding below, so all three
            // observe the custom error simultaneously with valueMissing.
            const seen = [flags.valueMissing, flags.customError, flags.valid];
            seen.push(input.validationMessage);
            input.setCustomValidity('');
            seen.push(input.validity.valid, input.validity.customError, input.validationMessage);
            document.querySelector('output').textContent = seen.join('|');
        </script></body>"#,
    );
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "true|true|false|Custom problem.|false|false|Please fill out this field."
    );
}

#[test]
fn email_and_url_type_checks_follow_the_platform_grammar() {
    let (dom, _) = check(
        r#"<body><input type=email id=a><input type=email multiple id=b><input type=url id=c><output></output><script>
            const [single, multi, url] = ['a', 'b', 'c'].map(id => document.getElementById(id));
            const seen = [];
            single.value = 'user@intranet'; seen.push(single.validity.typeMismatch);
            single.value = 'a@b@c'; seen.push(single.validity.typeMismatch);
            single.value = ''; seen.push(single.validity.valid);
            multi.value = 'a@b, c@d'; seen.push(multi.validity.typeMismatch);
            multi.value = 'a@b, nope'; seen.push(multi.validity.typeMismatch);
            url.value = 'https://example.com/x'; seen.push(url.validity.typeMismatch);
            url.value = 'mailto:someone@example.com'; seen.push(url.validity.typeMismatch);
            url.value = 'not a url'; seen.push(url.validity.typeMismatch);
            url.value = ''; seen.push(url.validity.valid);
            document.querySelector('output').textContent = seen.join('|');
        </script></body>"#,
    );
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "false|true|true|false|true|false|false|true|true"
    );
}

#[test]
fn pattern_uses_unicode_anchored_matching_with_type_gates() {
    let (dom, _) = check(
        r#"<body><input pattern="[a-z]+"><input pattern="(["><input type=number pattern="[0-9]+"><output></output><script>
            const [text, broken, numeric] = document.querySelectorAll('input');
            const seen = [];
            text.value = 'abc'; seen.push(text.validity.patternMismatch);
            text.value = 'ab1'; seen.push(text.validity.patternMismatch);
            text.value = ''; seen.push(text.validity.valid);
            broken.value = 'anything'; seen.push(broken.validity.patternMismatch, broken.validity.valid);
            numeric.value = '12a'; seen.push(numeric.validity.patternMismatch, numeric.validity.valid);
            text.pattern = '[\\p{Letter}]+';
            text.value = 'Äöü'; seen.push(text.validity.patternMismatch);
            document.querySelector('output').textContent = seen.join('|');
        </script></body>"#,
    );
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "false|true|true|false|true|false|true|false"
    );
}

#[test]
fn numeric_bounds_cover_absence_invalid_zero_and_reversal() {
    let (dom, _) = check(
        r#"<body>
            <input type=number id=a min="5" max="10"><input type=number id=b min="oops" max="10">
            <input type=number id=c min="10" max="5"><input type=number id=d>
            <output></output><script>
            const [a, b, c, d] = ['a', 'b', 'c', 'd'].map(id => document.getElementById(id));
            const seen = [];
            a.value = '3'; seen.push(a.validity.rangeUnderflow, a.validity.rangeOverflow, a.validity.valid);
            a.value = '12'; seen.push(a.validity.rangeOverflow, a.validationMessage);
            b.value = '3'; seen.push(b.validity.rangeUnderflow, b.validity.valid);
            c.value = '7'; seen.push(c.validity.rangeUnderflow, c.validity.rangeOverflow, c.validity.valid);
            c.value = '3'; seen.push(c.validity.rangeUnderflow, c.validity.rangeOverflow);
            d.value = '-1e2'; seen.push(d.validity.valid, d.value);
            d.value = 'Infinity'; seen.push(JSON.stringify(d.value), d.validity.valid);
            document.querySelector('output').textContent = seen.join('|');
        </script></body>"#,
    );
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "true|false|false|true|Value is above the maximum.|false|true|true|true|false|true|false|true|-1e2|\"\"|true"
    );
}

#[test]
fn value_as_number_and_steppers_follow_spec_errors() {
    let (dom, _) = check(
        r#"<body><input type=number><input type=range min="0" max="10" step="3"><input type=text><output></output><script>
            const [number, range, text] = document.querySelectorAll('input');
            const seen = [];
            seen.push(Number.isNaN(number.valueAsNumber));
            number.value = '12.5'; seen.push(number.valueAsNumber);
            number.valueAsNumber = 0.1 + 0.2; seen.push(number.value);
            number.valueAsNumber = NaN; seen.push(JSON.stringify(number.value));
            try { number.valueAsNumber = Infinity; seen.push('no-throw'); }
            catch (error) { seen.push(error instanceof TypeError); }
            try { text.valueAsNumber = 1; seen.push('no-throw'); }
            catch (error) { seen.push(error.name); }
            seen.push(text.valueAsNumber !== text.valueAsNumber, text.valueAsDate);
            try { text.valueAsDate = new Date(); seen.push('no-throw'); }
            catch (error) { seen.push(error.name); }
            number.value = '5'; number.stepUp(); seen.push(number.value);
            number.stepDown(2); seen.push(number.value);
            range.value = '9'; range.stepDown(); seen.push(range.value);
            try { number.stepUp.call(text); seen.push('no-throw'); }
            catch (error) { seen.push(error.name); }
            document.querySelector('output').textContent = seen.join('|');
        </script></body>"#,
    );
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "true|12.5|0.30000000000000004|\"\"|true|InvalidStateError|true||InvalidStateError|6|4|6|InvalidStateError"
    );
}

#[test]
fn will_validate_covers_eligibility_matrix() {
    let (dom, _) = check(
        r#"<body><form><fieldset disabled><legend><input id=in-legend></legend><input id=in-fieldset></fieldset>
            <input id=direct disabled><input type=hidden required id=hidden><input type=submit id=submit>
            <input type=reset id=reset required><input type=button id=button>
            <button id=btn-submit></button><button type=button id=btn-button></button>
            <datalist><select id=in-data></select></datalist>
            <output id=out></output><fieldset id=fs></fieldset>
            </form><output></output><script>
            const ids = ['in-legend', 'in-fieldset', 'direct', 'hidden', 'submit', 'reset', 'button',
                'btn-submit', 'btn-button', 'in-data', 'out', 'fs'];
            const seen = ids.map(id => document.getElementById(id).willValidate);
            document.querySelectorAll('output')[1].textContent = seen.join('|');
        </script></body>"#,
    );
    assert_eq!(
        dom.elements_named("output").nth(1).unwrap().text_content(),
        "true|false|false|false|true|false|false|true|false|false|false|false"
    );
}

#[test]
fn check_and_report_events_are_untrusted_and_ordered() {
    let (dom, _) = check(
        r#"<body><form><input required><input required value="ok"></form><output></output><script>
            const form = document.querySelector('form');
            const [first, second] = form.querySelectorAll('input');
            const log = [];
            document.addEventListener('invalid', event => log.push(
                'doc-capture:' + event.target.tagName + ':' + event.isTrusted + ':' + event.bubbles + ':' + event.cancelable), true);
            form.addEventListener('invalid', () => log.push('form-bubble'));
            first.addEventListener('invalid', event => { log.push('first'); event.preventDefault(); });
            const checked = form.checkValidity();
            const reported = form.reportValidity();
            // Cancellation claims responsibility (positive result) without
            // making the invalid data valid.
            log.push('checked=' + checked, 'reported=' + reported,
                'still-invalid=' + first.validity.valid,
                'active=' + (document.activeElement === first));
            document.querySelector('output').textContent = log.join('|');
        </script></body>"#,
    );
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "doc-capture:INPUT:false:false:true|first|doc-capture:INPUT:false:false:true|first|checked=false|reported=true|still-invalid=false|active=false"
    );
}

#[test]
fn report_validity_focuses_the_first_unhandled_control() {
    let (dom, _) = check(
        r#"<body><form><input required><input required></form><output></output><script>
            const form = document.querySelector('form');
            const [first, second] = form.querySelectorAll('input');
            const reported = form.reportValidity();
            const focusedFirst = document.activeElement === first;
            const controlReported = second.reportValidity();
            document.querySelector('output').textContent =
                [reported, focusedFirst, controlReported].join('|');
        </script></body>"#,
    );
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "false|true|false"
    );
}

#[test]
fn custom_validity_barred_elements_and_clones() {
    let (dom, _) = check(
        r#"<body><output></output><button type=submit></button><output id=log></output><script>
            const field = document.querySelector('button');
            const out = document.querySelector('output');
            field.setCustomValidity('Nope.');
            const clone = field.cloneNode();
            const seen = [field.validity.customError, field.validity.valid, field.validationMessage,
                clone.validity.customError, clone.validationMessage];
            field.setCustomValidity('');
            seen.push(field.validity.valid);
            out.setCustomValidity('Stored.');
            seen.push(out.validity.valid, out.validationMessage);
            document.getElementById('log').textContent = seen.join('|');
        </script></body>"#,
    );
    assert_eq!(
        dom.elements_named("output").nth(1).unwrap().text_content(),
        "true|false|Nope.|false||true|false|"
    );
}
