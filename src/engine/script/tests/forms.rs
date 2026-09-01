use super::*;

#[test]
fn button_type_reflects_the_enumerated_content_attribute() {
    let (dom, outcome) = execute_html(
        r#"<body><button></button><output></output><script>
            const button = document.querySelector('button');
            const values = [button.type];
            for (const type of ['reset', 'button', 'submit']) {
                button.type = type.toUpperCase();
                values.push(button.type, button.getAttribute('type'));
            }
            button.type = 'invalid';
            values.push(button.type, button.getAttribute('type'));
            button.type = '';
            values.push(button.type, button.getAttribute('type'));
            document.querySelector('output').textContent = values.join('|');
        </script></body>"#,
    );

    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "submit|reset|RESET|button|BUTTON|submit|SUBMIT|submit|invalid|submit|"
    );
}

#[test]
fn datalist_options_are_live_and_bar_descendant_controls_from_validation() {
    let (dom, outcome) = execute_html(
        r#"<body><datalist id="choices"><select required>
              <option label="One" value="1"></option><option> Two
                Words </option><option>
              </option>
            </select></datalist><output></output><script>
            const datalist = document.querySelector('datalist');
            const select = document.querySelector('select');
            const options = datalist.options;
            const option = document.createElement('option');
            option.textContent = 'Three';
            datalist.appendChild(option);
            document.querySelector('output').textContent = [
                options instanceof HTMLCollection, options.length,
                options.item(0).label, options.item(0).value,
                options.item(1).label, options.item(1).value,
                options.item(2).label, options.item(2).value,
                options.item(3) instanceof HTMLOptionElement,
                options.item(3).label, options.item(3).value,
                select.willValidate, select.checkValidity()
            ].join('|');
        </script></body>"#,
    );

    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "true|4|One|1|Two Words|Two Words|||true|Three|Three|false|true"
    );
}
