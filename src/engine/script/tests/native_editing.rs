use super::*;

#[test]
fn native_edits_do_not_invoke_author_value_setters() {
    for tag in ["input", "textarea"] {
        let dom = dom::parse_with_scripting(
            &format!(
                r#"<{tag} id=query></{tag}><output></output><script>
            const field=document.getElementById('query');
            const prototype=field instanceof HTMLInputElement ? HTMLInputElement.prototype
                : field instanceof HTMLTextAreaElement ? HTMLTextAreaElement.prototype : Element.prototype;
            const descriptor=Object.getOwnPropertyDescriptor(prototype,'value');
            let tracked='', setterCalls=0;
            Object.defineProperty(field,'value',{{
                get(){{return descriptor.get.call(this)}},
                set(value){{setterCalls++;tracked=String(value);descriptor.set.call(this,value)}}
            }});
            field.addEventListener('input',event=>{{
                const changed=field.value!==tracked;
                document.querySelector('output').textContent=[changed,setterCalls,field.value,event.isTrusted].join('|');
            }});
            // Replacing the prototype setter later must not intercept internal user editing.
            Object.defineProperty(prototype,'value',{{...descriptor,set(){{throw Error('author prototype setter')}}}});
        </script>"#
            ),
            true,
        );
        let script = dom.elements_named("script").next().unwrap();
        let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
        let result = runtime.execute_initial(&[ScriptInput {
            source_url: "https://example.com/".into(),
            code: script.text_content(),
            node: script,
            kind: ScriptKind::Classic,
            fetch_options: ScriptFetchOptions::for_kind(ScriptKind::Classic),
            finish_lifecycle: true,
        }]);
        assert!(result.errors.is_empty(), "{:?}", result.errors);
        let result = runtime.dispatch_user_input(UserInputEvent::Text {
            target: dom.elements_named(tag).next().unwrap(),
            value: "user search".into(),
            selection_start: 11,
            selection_end: 11,
        });
        assert!(
            result.outcome.errors.is_empty(),
            "{:?}",
            result.outcome.errors
        );
        assert_eq!(
            dom.elements_named("output").next().unwrap().text_content(),
            "true|0|user search|true",
            "{tag}"
        );
    }
}
