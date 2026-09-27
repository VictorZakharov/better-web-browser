use super::*;

fn check(script: &str) {
    let (_, outcome) = execute_html(&format!("<body><script>{script}</script>"));
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
}

#[test]
fn template_shadow_attributes_reflect_their_content_attributes() {
    check(
        r#"
        const template = document.createElement('template');
        if (!(template instanceof HTMLTemplateElement) || template.shadowRootMode !== '' ||
            template.shadowRootSlotAssignment !== 'named' || template.shadowRootDelegatesFocus ||
            template.shadowRootSerializable || template.shadowRootClonable ||
            template.shadowRootCustomElementRegistry !== '' || template.htmlFor !== '')
            throw Error('missing-value defaults');

        template.shadowRootMode = 'open';
        template.shadowRootSlotAssignment = 'manual';
        template.shadowRootDelegatesFocus = true;
        template.shadowRootSerializable = true;
        template.shadowRootClonable = true;
        template.shadowRootCustomElementRegistry = '';
        template.htmlFor = 'marker';
        if (template.getAttribute('shadowrootmode') !== 'open' ||
            template.getAttribute('shadowrootslotassignment') !== 'manual' ||
            template.getAttribute('shadowrootdelegatesfocus') !== '' ||
            template.getAttribute('shadowrootserializable') !== '' ||
            template.getAttribute('shadowrootclonable') !== '' ||
            template.getAttribute('shadowrootcustomelementregistry') !== '' ||
            template.getAttribute('for') !== 'marker') throw Error('IDL to content');

        template.removeAttribute('shadowrootmode');
        template.setAttribute('shadowrootslotassignment', 'named');
        template.removeAttribute('shadowrootdelegatesfocus');
        template.removeAttribute('shadowrootserializable');
        template.removeAttribute('shadowrootclonable');
        template.setAttribute('shadowrootcustomelementregistry', 'scoped');
        template.setAttribute('for', 'next');
        if (template.shadowRootMode !== '' || template.shadowRootSlotAssignment !== 'named' ||
            template.shadowRootDelegatesFocus || template.shadowRootSerializable ||
            template.shadowRootClonable ||
            template.shadowRootCustomElementRegistry !== 'scoped' || template.htmlFor !== 'next')
            throw Error('content to IDL');
    "#,
    );
}

#[test]
fn enumerated_shadow_attributes_return_canonical_known_values_without_rewriting_content() {
    check(
        r#"
        const template = document.createElement('template');
        template.setAttribute('shadowrootmode', 'ClOsEd');
        template.setAttribute('shadowrootslotassignment', 'MaNuAl');
        if (template.shadowRootMode !== 'closed' ||
            template.shadowRootSlotAssignment !== 'manual') throw Error('ASCII case matching');
        template.shadowRootMode = 'OPEN';
        template.shadowRootSlotAssignment = 'MANUAL';
        if (template.getAttribute('shadowrootmode') !== 'OPEN' ||
            template.getAttribute('shadowrootslotassignment') !== 'MANUAL' ||
            template.shadowRootMode !== 'open' ||
            template.shadowRootSlotAssignment !== 'manual') throw Error('setter preserves value');

        template.shadowRootMode = 'invalid';
        template.shadowRootSlotAssignment = 'invalid';
        if (template.getAttribute('shadowrootmode') !== 'invalid' ||
            template.shadowRootMode !== '' ||
            template.getAttribute('shadowrootslotassignment') !== 'invalid' ||
            template.shadowRootSlotAssignment !== 'named') throw Error('invalid-value defaults');
        template.setAttribute('shadowrootmode', ' open ');
        template.setAttribute('shadowrootslotassignment', ' manual ');
        if (template.shadowRootMode !== '' || template.shadowRootSlotAssignment !== 'named')
            throw Error('enumerated values must not be whitespace-trimmed');
    "#,
    );
}

#[test]
fn boolean_shadow_attributes_use_presence_and_web_idl_boolean_conversion() {
    check(
        r#"
        const template = document.createElement('template');
        for (const [property, attribute] of [
            ['shadowRootDelegatesFocus', 'shadowrootdelegatesfocus'],
            ['shadowRootSerializable', 'shadowrootserializable'],
            ['shadowRootClonable', 'shadowrootclonable']
        ]) {
            template.setAttribute(attribute, 'false');
            if (template[property] !== true) throw Error(property + ' presence');
            template[property] = 0;
            if (template.hasAttribute(attribute) || template[property] !== false)
                throw Error(property + ' removal');
            template[property] = 'false';
            if (template.getAttribute(attribute) !== '' || template[property] !== true)
                throw Error(property + ' truthy conversion');
        }
        template.shadowRootCustomElementRegistry = false;
        if (template.getAttribute('shadowrootcustomelementregistry') !== 'false' ||
            template.shadowRootCustomElementRegistry !== 'false')
            throw Error('custom registry is a DOMString reflector, not a boolean one');
    "#,
    );
}
