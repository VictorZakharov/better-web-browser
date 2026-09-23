use super::*;

fn output(html: &str) -> String {
    let (dom, outcome) = execute_html(html);
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    dom.elements_named("output").next().unwrap().text_content()
}

#[test]
fn temporal_values_have_independent_live_state_and_reset_to_default() {
    assert_eq!(
        output(
            r#"<form><input type=date value=2024-02-29></form><output></output><script>
            const form = document.querySelector('form'), input = form.firstElementChild;
            const seen = [input.value, input.valueAsDate.toISOString()];
            input.value = '2025-03-01';
            input.defaultValue = '2020-01-02';
            seen.push(input.value, input.valueAsDate.toISOString());
            form.reset();
            seen.push(input.value, input.valueAsDate.toISOString());
            document.querySelector('output').textContent = seen.join('|');
            </script>"#
        ),
        "2024-02-29|2024-02-29T00:00:00.000Z|2025-03-01|2025-03-01T00:00:00.000Z|2020-01-02|2020-01-02T00:00:00.000Z"
    );
}

#[test]
fn date_month_week_and_time_use_utc_and_setter_type_checks() {
    assert_eq!(
        output(
            r#"<input type=date><input type=month><input type=week><input type=time>
            <input type=datetime-local><output></output><script>
            const [date, month, week, time, local] = document.querySelectorAll('input');
            const seen = [];
            date.valueAsDate = new Date('2024-02-29T23:59:00Z');
            month.valueAsDate = new Date('2024-02-29T23:59:00Z');
            week.valueAsDate = new Date('2021-01-01T12:00:00Z');
            time.valueAsDate = new Date('2024-02-29T12:34:56.120Z');
            seen.push(date.value, month.value, week.value, time.value);
            seen.push(week.valueAsDate.toISOString(), time.valueAsDate.toISOString());
            date.valueAsDate = new Date(NaN);
            seen.push(date.value);
            try { date.valueAsDate = 123; } catch (error) { seen.push(error.name); }
            try { local.valueAsDate = new Date(); } catch (error) { seen.push(error.name); }
            seen.push(String(local.valueAsDate));
            document.querySelector('output').textContent = seen.join('|');
            </script>"#
        ),
        "2024-02-29|2024-02|2020-W53|12:34:56.12|2020-12-28T00:00:00.000Z|1970-01-01T12:34:56.120Z||TypeError|InvalidStateError|null"
    );
}

#[test]
fn color_input_sanitizes_to_lowercase_opaque_hex_and_keeps_live_value() {
    assert_eq!(
        output(
            r#"<form><input type=color value='#ABCDEF'></form><output></output><script>
            const form = document.querySelector('form'), input = form.firstElementChild;
            const seen = [input.value];
            input.value = '#123AbC';
            input.defaultValue = '#fedcba';
            seen.push(input.value);
            input.value = 'red';
            seen.push(input.value);
            form.reset();
            seen.push(input.value);
            document.querySelector('output').textContent = seen.join('|');
            </script>"#
        ),
        "#abcdef|#123abc|#000000|#fedcba"
    );
}

#[test]
fn temporal_value_as_number_uses_month_counts_and_timezone_free_milliseconds() {
    assert_eq!(
        output(
            r#"<input type=date><input type=month><input type=week>
            <input type=time><input type=datetime-local><input type=text>
            <output></output><script>
            const [date, month, week, time, local, text] =
                document.querySelectorAll('input');
            const seen = [];
            date.valueAsNumber = Date.UTC(2024, 1, 29);
            month.valueAsNumber = 650;
            week.valueAsNumber = Date.UTC(2020, 11, 28);
            time.valueAsNumber = 45296000;
            local.valueAsNumber = Date.UTC(2024, 1, 29, 12, 34);
            seen.push(date.value, month.value, week.value, time.value, local.value);
            seen.push(date.valueAsNumber === Date.UTC(2024, 1, 29),
                month.valueAsNumber, week.valueAsNumber === Date.UTC(2020, 11, 28),
                time.valueAsNumber, local.valueAsNumber === Date.UTC(2024, 1, 29, 12, 34));
            time.valueAsNumber = NaN;
            seen.push(time.value, Number.isNaN(time.valueAsNumber));
            try { local.valueAsNumber = Infinity; } catch (error) { seen.push(error.name); }
            try { text.valueAsNumber = 1; } catch (error) { seen.push(error.name); }
            document.querySelector('output').textContent = seen.join('|');
            </script>"#
        ),
        "2024-02-29|2024-03|2020-W53|12:34:56|2024-02-29T12:34|true|650|true|45296000|true||true|TypeError|InvalidStateError"
    );
}
