use super::*;
use crate::engine::Page;

const HTML: &str = "<main><span id=target>text</span></main>";
const URL: &str = "https://example.com/";

fn sheet(name: &str, color: &str, position: &str) -> (String, String) {
    (
        format!("{URL}{name}.css"),
        format!("#target{{color:{color}}}main{{position:{position}}}"),
    )
}

#[test]
fn external_source_order_updates_and_removal_match_the_layout_cascade() {
    let dom = crate::engine::dom::parse(HTML);
    let target = dom.elements_named("span").next().unwrap();
    let main = dom.elements_named("main").next().unwrap();
    let body = dom.elements_named("body").next().unwrap();
    let mut state = HostState::new(
        dom.document.clone(),
        URL,
        "UTF-8",
        Rc::new(module_loader::WebModuleLoader::new()),
    );
    let red = sheet("red", "red", "relative");
    let blue = sheet("blue", "blue", "static");
    let updated_red = sheet("red", "green", "relative");
    let version = dom.document.document_mutation_version();
    let cases = [
        (vec![red.clone(), blue.clone()], "rgb(0, 0, 255)", false),
        (vec![blue.clone(), red.clone()], "rgb(255, 0, 0)", true),
        (vec![red.clone(), blue.clone(), red], "rgb(255, 0, 0)", true),
        (vec![blue.clone(), updated_red], "rgb(0, 128, 0)", true),
        (vec![blue], "rgb(0, 0, 255)", false),
        (vec![], "rgb(0, 0, 0)", false),
    ];
    for (sources, expected_color, main_is_positioned) in cases {
        state.layout_geometry_initialized = true;
        state.replace_document_stylesheets(&sources);
        assert_eq!(state.stylesheet_sources, sources);
        assert!(state.computed_styles.is_none());
        assert!(state.offset_parent_styles.is_none());
        assert!(!state.layout_geometry_initialized);
        assert_eq!(dom.document.document_mutation_version(), version);

        let mut page = Page::parse(HTML, URL);
        for (url, source) in &sources {
            page.add_stylesheet_from(url, source.clone());
        }
        let layout_styles = page.style_for_viewport(800.0, 600.0);
        let layout_target = page.dom.elements_named("span").next().unwrap();
        let layout_color = resolved_property_value(layout_styles.get(&layout_target), "color");
        let computed_color = state.computed_style_property(&target, "color");
        assert_eq!(computed_color.as_deref(), Some(expected_color));
        assert_eq!(computed_color, layout_color);
        assert_eq!(
            state.offset_parent(&target).unwrap().id(),
            if main_is_positioned {
                main.id()
            } else {
                body.id()
            },
        );

        // An identical ordered snapshot needs no cache or geometry invalidation.
        state.layout_geometry_initialized = true;
        state.replace_document_stylesheets(&sources);
        assert!(state.computed_styles.is_some());
        assert!(state.offset_parent_styles.is_some());
        assert!(state.layout_geometry_initialized);
    }
}

#[test]
fn cssom_source_lookup_keeps_last_resource_value_without_deduplicating_cascade_order() {
    let dom = crate::engine::dom::parse(HTML);
    let mut state = HostState::new(
        dom.document.clone(),
        URL,
        "UTF-8",
        Rc::new(module_loader::WebModuleLoader::new()),
    );
    let old = sheet("same", "red", "relative");
    let current = sheet("same", "blue", "static");
    state.replace_document_stylesheets(&[old.clone(), current.clone()]);
    let args = [
        JsValue::undefined(),
        binding_helpers::js_string(current.0.clone()),
    ];
    let value = cssom_host::cssom_host_call("stylesheetSource", &args, &mut state).unwrap();
    assert!(matches!(value, Some(JsValue::String(value)) if value == current.1));
    assert_eq!(state.stylesheet_sources.len(), 2);
    state.replace_document_stylesheets(std::slice::from_ref(&old));
    let value = cssom_host::cssom_host_call("stylesheetSource", &args, &mut state).unwrap();
    assert!(matches!(value, Some(JsValue::String(value)) if value == old.1));
    state.replace_document_stylesheets(&[]);
    let value = cssom_host::cssom_host_call("stylesheetSource", &args, &mut state).unwrap();
    assert!(matches!(value, Some(JsValue::Null)));
}
