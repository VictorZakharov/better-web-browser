use super::*;

#[test]
fn svg_nodes_expose_the_namespace_specific_interface_hierarchy() {
    let (dom, outcome) = execute_html(
        r#"<!doctype html><body><output>no</output><svg id="root"><g id="group"><circle id="circle" class="initial"></circle></g></svg><script>
            const root = document.getElementById('root');
            const group = document.getElementById('group');
            const circle = document.getElementById('circle');
            const liveClassName = circle.className;
            liveClassName.baseVal = 'updated';
            const created = document.createElementNS('http://www.w3.org/2000/svg', 'path');
            const nested = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
            group.appendChild(nested);
            nested.appendChild(created);
            const accepted =
                root instanceof SVGSVGElement && root instanceof SVGElement && root instanceof Element &&
                !(root instanceof HTMLElement) && group instanceof SVGElement &&
                circle instanceof SVGElement && circle.className === liveClassName &&
                liveClassName instanceof SVGAnimatedString && liveClassName.baseVal === 'updated' &&
                liveClassName.animVal === 'updated' && circle.getAttribute('class') === 'updated' &&
                root.ownerSVGElement === null && group.ownerSVGElement === root &&
                group.viewportElement === root && nested instanceof SVGSVGElement &&
                nested.ownerSVGElement === root && created.ownerSVGElement === nested;
            if (accepted) document.querySelector('output').textContent = 'yes';
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "yes"
    );
}

#[test]
fn web_component_node_interfaces_expose_the_standard_prototype_hierarchy() {
    let (_, outcome) = execute_html(
        r#"<!doctype html><script>
            if (!(CDATASection.prototype instanceof Text) ||
                !(ProcessingInstruction.prototype instanceof CharacterData))
                throw new Error('missing CharacterData interface hierarchy');
        </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
}
