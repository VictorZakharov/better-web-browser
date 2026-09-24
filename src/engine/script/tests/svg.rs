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

#[test]
fn svg_color_matrix_reflects_live_filter_attributes_and_enumerations() {
    let (dom, outcome) = execute_html(
        r#"<!doctype html><body><output>no</output>
        <svg><filter id="filter"><feColorMatrix id="matrix" in="SourceGraphic" type="saturate" values="0.5"/></filter></svg>
        <script>
          const matrix = document.getElementById('matrix');
          const type = matrix.type;
          const values = matrix.values.baseVal;
          const first = values.getItem(0);
          first.value = 0.25;
          type.baseVal = SVGFEColorMatrixElement.SVG_FECOLORMATRIX_TYPE_HUEROTATE;
          const created = document.createElementNS('http://www.w3.org/2000/svg', 'feColorMatrix');
          const ok = matrix instanceof SVGFEColorMatrixElement &&
            matrix instanceof SVGElement && matrix.in1.baseVal === 'SourceGraphic' &&
            type === matrix.type && type.animVal === type.baseVal &&
            type.baseVal === matrix.SVG_FECOLORMATRIX_TYPE_HUEROTATE &&
            matrix.getAttribute('type') === 'hueRotate' &&
            values !== matrix.values.animVal &&
            matrix.values.animVal.getItem(0).value === 0.25 &&
            first instanceof SVGNumber &&
            values.numberOfItems === 1 && values.getItem(0) === first &&
            matrix.getAttribute('values') === '0.25' &&
            created instanceof SVGFEColorMatrixElement &&
            created.type.baseVal === created.SVG_FECOLORMATRIX_TYPE_MATRIX &&
            SVGFEColorMatrixElement.SVG_FECOLORMATRIX_TYPE_SATURATE === 2;
          if (ok) document.querySelector('output').textContent = 'yes';
          try { matrix.values.animVal.clear(); throw new Error('animated list was mutable'); }
          catch (error) { if (error.name !== 'NoModificationAllowedError') throw error; }
          try { matrix.values.animVal.getItem(0).value = 0.75;
              throw new Error('animated item was mutable'); }
          catch (error) { if (error.name !== 'NoModificationAllowedError') throw error; }
          const number = document.querySelector('svg').createSVGNumber();
          number.value = 0.8;
          values.appendItem(number);
          if (values.length !== 2 || values.getItem(1) !== number ||
              matrix.getAttribute('values') !== '0.25 0.8')
            throw new Error('SVGNumberList did not reflect a new number');
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "yes"
    );
}
