use super::*;

fn check_script(code: &str) {
    let (_, outcome) = execute_html(&format!("<!doctype html><body><script>{code}</script>"));
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
}

#[test]
fn static_unsafe_parser_builds_inert_about_blank_document_with_declarative_roots() {
    check_script(
        r#"
      const check = (ok, name) => { if (!ok) throw Error(name); };
      window.unsafeParsedRuns = 0;
      const source = '<!doctype html><x-card><template shadowrootmode=open shadowrootserializable>' +
          '<span>A &amp; B</span><script>window.unsafeParsedRuns++</scr' + 'ipt>' +
          '<button onclick="window.unsafeParsedRuns++">go</button></template></x-card>';
      const parsed = Document.parseHTMLUnsafe(source);
      check(parsed !== document && parsed instanceof Document, 'new document');
      check(parsed.defaultView === null && parsed.readyState === 'complete', 'inert document');
      check(parsed.URL === 'about:blank' && parsed.baseURI === 'about:blank', 'about:blank URL');
      check(parsed.contentType === 'text/html' && parsed.characterSet === 'UTF-8' &&
          parsed.compatMode === 'CSS1Compat', 'document metadata');
      check(parsed.customElementRegistry === null, 'detached null registry');
      const host = parsed.querySelector('x-card'), root = host.shadowRoot;
      check(root instanceof ShadowRoot && root.host === host && root.serializable,
          'declarative root');
      check(root.customElementRegistry === null && root.querySelector('span').textContent === 'A & B',
          'root registry and content');
      check(root.querySelector('script') && root.querySelector('button').getAttribute('onclick'),
          'unsafe markup retained');
      check(window.unsafeParsedRuns === 0, 'script ran during parsing');
      const serialized = host.getHTML({serializableShadowRoots:true});
      check(serialized.includes('shadowrootmode="open"') &&
          !serialized.includes('shadowrootcustomelementregistry'), 'null-root serialization');
      const roundTrip = Document.parseHTMLUnsafe('<x-card>' + serialized + '</x-card>');
      check(roundTrip.querySelector('x-card').shadowRoot.querySelector('span').textContent === 'A & B',
          'getHTML round trip');
      const inert = new DOMParser().parseFromString(source, 'text/html');
      check(inert.querySelector('x-card').shadowRoot === null &&
          inert.querySelector('x-card').firstElementChild.localName === 'template',
          'DOMParser stays opt-out');
      document.body.append(document.adoptNode(host));
      check(window.unsafeParsedRuns === 0 && root.customElementRegistry === customElements,
          'script stayed inert after adoption');
    "#,
    );
}

#[test]
fn static_unsafe_parser_validates_arguments_and_rejects_unsupported_sanitizer() {
    check_script(
        r#"
      const expect = (action, name) => {
          try { action(); } catch (error) {
              if (error.name === name) return;
              throw error;
          }
          throw Error('missing ' + name);
      };
      expect(() => Document.parseHTMLUnsafe(), 'TypeError');
      expect(() => Document.parseHTMLUnsafe(Symbol()), 'TypeError');
      expect(() => Document.parseHTMLUnsafe('<p>ok', 1), 'TypeError');
      expect(() => Document.parseHTMLUnsafe('<p>ok', {sanitizer:{}}), 'NotSupportedError');
      const descriptor = Object.getOwnPropertyDescriptor(Document, 'parseHTMLUnsafe');
      if (!descriptor.writable || !descriptor.enumerable || !descriptor.configurable ||
          Document.parseHTMLUnsafe.length !== 1)
          throw Error('static Web IDL operation descriptor');
      const conversions = [];
      expect(() => Document.parseHTMLUnsafe({toString() {
          conversions.push('input');
          return '<p>ok';
      }}, {get sanitizer() {
          conversions.push('options');
          return {};
      }}), 'NotSupportedError');
      if (conversions.join(',') !== 'input,options')
          throw Error('Web IDL argument conversion order');
      const parsed = Document.parseHTMLUnsafe('<p>ok', {sanitizer:undefined});
      if (parsed.body.firstElementChild.textContent !== 'ok')
          throw Error('empty options dictionary');
    "#,
    );
}

#[test]
fn detached_unsafe_parse_defers_custom_element_upgrades_until_adoption() {
    check_script(
        r#"
      let constructed = 0;
      class DetachedElement extends HTMLElement {
          constructor() { super(); constructed++; }
      }
      customElements.define('x-detached', DetachedElement);
      const parsed = Document.parseHTMLUnsafe(
          '<x-detached><template shadowrootmode=open>' +
          '<x-detached></x-detached></template></x-detached>');
      const host = parsed.querySelector('x-detached');
      const nested = host.shadowRoot.querySelector('x-detached');
      if (constructed !== 0 || host instanceof DetachedElement ||
          nested instanceof DetachedElement || host.customElementRegistry !== null ||
          nested.customElementRegistry !== null)
          throw Error('detached null-registry tree upgraded early');
      document.body.append(document.adoptNode(host));
      if (constructed !== 2 || !(host instanceof DetachedElement) ||
          !(nested instanceof DetachedElement))
          throw Error('adopted shadow-inclusive tree did not upgrade');
    "#,
    );
}
