use super::*;

fn result(dom: &super::super::super::dom::Dom) -> Option<String> {
    dom.elements_named("body")
        .next()
        .and_then(|body| body.attr("data-result"))
}

#[test]
fn parsed_null_registry_survives_removal_and_same_document_moves() {
    let (dom, outcome) = execute_html(
        r#"<body>
        <div id="remove-host"><template shadowrootmode="open" shadowrootcustomelementregistry>
            <x-provenance id="removed"><span id="nested"></span></x-provenance>
        </template></div>
        <div id="append-host"><template shadowrootmode="open" shadowrootcustomelementregistry>
            <x-provenance id="appended"></x-provenance>
        </template></div>
        <div id="insert-host"><template shadowrootmode="open" shadowrootcustomelementregistry>
            <x-provenance id="inserted"></x-provenance>
        </template></div>
        <script>
        const removeRoot = document.querySelector('#remove-host').shadowRoot;
        const appendRoot = document.querySelector('#append-host').shadowRoot;
        const insertRoot = document.querySelector('#insert-host').shadowRoot;
        // Do not read registry getters before mutation: parser-created wrappers
        // must retain their creation registry even if it was not yet cached in JS.
        const removed = removeRoot.querySelector('#removed');
        const nested = removed.querySelector('#nested');
        const appended = appendRoot.querySelector('#appended');
        const inserted = insertRoot.querySelector('#inserted');
        removeRoot.removeChild(removed);
        document.body.appendChild(appended);
        document.body.insertBefore(inserted, document.body.firstChild);
        const preserved = [removed, nested, appended, inserted].every(element =>
            element.customElementRegistry === null);
        class Global extends HTMLElement {}
        customElements.define('x-provenance', Global);
        const notGloballyUpgraded = [removed, appended, inserted].every(element =>
            !(element instanceof Global));
        document.body.setAttribute('data-result', preserved && notGloballyUpgraded ? 'pass' : 'fail');
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(result(&dom).as_deref(), Some("pass"));
}

#[test]
fn parsed_registry_survives_replacement_and_same_document_adoption() {
    let (dom, outcome) = execute_html(
        r#"<body>
        <div id="text-host"><template shadowrootmode="open" shadowrootcustomelementregistry>
            <x-provenance id="text-child"></x-provenance>
        </template></div>
        <div id="html-host"><template shadowrootmode="open" shadowrootcustomelementregistry>
            <x-provenance id="html-child"></x-provenance>
        </template></div>
        <div id="adopt-host"><template shadowrootmode="open" shadowrootcustomelementregistry>
            <x-provenance id="adopt-child"></x-provenance>
        </template></div>
        <div id="scoped-host"><template shadowrootmode="open" shadowrootcustomelementregistry>
            <x-provenance id="scoped-child"></x-provenance>
        </template></div>
        <script>
        const textRoot = document.querySelector('#text-host').shadowRoot;
        const htmlRoot = document.querySelector('#html-host').shadowRoot;
        const adoptRoot = document.querySelector('#adopt-host').shadowRoot;
        const scopedRoot = document.querySelector('#scoped-host').shadowRoot;
        const textChild = textRoot.firstElementChild;
        const htmlChild = htmlRoot.firstElementChild;
        const adoptChild = adoptRoot.firstElementChild;
        const scopedChild = scopedRoot.firstElementChild;
        textRoot.textContent = '';
        htmlRoot.innerHTML = '';
        document.adoptNode(adoptChild);
        const preservedNull = [textChild, htmlChild, adoptChild].every(element =>
            element.customElementRegistry === null);
        const scoped = new CustomElementRegistry();
        scoped.initialize(scopedRoot);
        scopedRoot.removeChild(scopedChild);
        const preservedScoped = scopedChild.customElementRegistry === scoped;
        document.body.setAttribute('data-result', preservedNull && preservedScoped ? 'pass' : 'fail');
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(result(&dom).as_deref(), Some("pass"));
}
