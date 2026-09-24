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

#[test]
fn svg_filter_primitives_reflect_attributes_used_by_the_rasterizer() {
    let (dom, outcome) = execute_html(
        r#"<!doctype html><body><output>no</output><svg>
          <filter><feGaussianBlur id="blur" stdDeviation="2 3"/>
            <feOffset id="offset" dx="4" dy="5" result="shift"/>
            <feComposite id="composite" in="SourceGraphic" in2="shift" operator="in"/>
            <feBlend id="blend" mode="multiply"/><feFlood id="flood"/>
            <feMerge id="merge"><feMergeNode id="node" in="shift"/></feMerge>
          </filter></svg><script>
          const blur = document.getElementById('blur');
          const offset = document.getElementById('offset');
          const composite = document.getElementById('composite');
          const blend = document.getElementById('blend');
          blur.setStdDeviation(1, 4);
          offset.dx.baseVal = 8;
          composite.operator.baseVal = composite.SVG_FECOMPOSITE_OPERATOR_OUT;
          blend.mode.baseVal = blend.SVG_FEBLEND_MODE_SCREEN;
          offset.result.baseVal = 'moved';
          const checks = [
            blur instanceof SVGFEGaussianBlurElement,
            blur.stdDeviationX.baseVal === 1 && blur.stdDeviationY.baseVal === 4,
            blur.stdDeviationX instanceof SVGAnimatedNumber,
            blur.getAttribute('stdDeviation') === '1 4',
            offset instanceof SVGFEOffsetElement && offset.dx.baseVal === 8,
            offset.dy.baseVal === 5 && offset.getAttribute('result') === 'moved',
            composite instanceof SVGFECompositeElement && composite.in1.baseVal === 'SourceGraphic',
            composite.in2.baseVal === 'shift' && composite.getAttribute('operator') === 'out',
            blend instanceof SVGFEBlendElement && blend.getAttribute('mode') === 'screen',
            document.getElementById('flood') instanceof SVGFEFloodElement,
            document.getElementById('merge') instanceof SVGFEMergeElement,
            document.getElementById('node') instanceof SVGFEMergeNodeElement,
            document.getElementById('node').in1.baseVal === 'shift'
          ];
          if (checks.every(Boolean)) document.querySelector('output').textContent = 'yes';
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "yes"
    );
}

#[test]
fn svg_filter_lengths_and_units_reflect_and_keep_animated_values_read_only() {
    let (dom, outcome) = execute_html(
        r#"<!doctype html><body><output>no</output>
        <svg width="200" height="100"><filter id="f"><feOffset id="o"/></filter></svg>
        <script>
          const filter = document.getElementById('f');
          const offset = document.getElementById('o');
          const width = filter.width;
          const x = filter.x.baseVal;
          const primitiveWidth = offset.width.baseVal;
          const checks = [
            filter instanceof SVGFilterElement,
            filter.filterUnits.baseVal === SVGUnitTypes.SVG_UNIT_TYPE_OBJECTBOUNDINGBOX,
            filter.primitiveUnits.baseVal === SVGUnitTypes.SVG_UNIT_TYPE_USERSPACEONUSE,
            width.baseVal instanceof SVGLength && width.baseVal.unitType === SVGLength.SVG_LENGTHTYPE_PERCENTAGE,
            width.baseVal.value === 1.2 && x.value === -0.1,
            primitiveWidth.value === 200 && width.animVal !== width.baseVal
          ];
          width.baseVal.newValueSpecifiedUnits(SVGLength.SVG_LENGTHTYPE_CM, 2.54);
          checks.push(Math.abs(width.baseVal.value - 96) < 0.001,
            filter.getAttribute('width') === '2.54cm');
          width.baseVal.convertToSpecifiedUnits(SVGLength.SVG_LENGTHTYPE_PX);
          checks.push(Math.abs(width.baseVal.valueInSpecifiedUnits - 96) < 0.001,
            width.animVal.valueAsString.endsWith('px'));
          filter.filterUnits.baseVal = SVGUnitTypes.SVG_UNIT_TYPE_USERSPACEONUSE;
          checks.push(filter.getAttribute('filterUnits') === 'userSpaceOnUse');
          try { width.animVal.value = 3; throw new Error('animated length was mutable'); }
          catch (error) { if (error.name !== 'NoModificationAllowedError') throw error; }
          if (checks.every(Boolean)) document.querySelector('output').textContent = 'yes';
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "yes"
    );
}
