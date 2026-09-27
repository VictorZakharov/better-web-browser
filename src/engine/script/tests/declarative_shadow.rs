use super::*;

fn check(markup: &str) {
    let (_, outcome) = execute_html(markup);
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
}

#[test]
fn parsed_roots_have_live_wrappers_without_exposing_closed_roots() {
    check(
        r#"<body>
        <div id=open><template shadowrootmode=open><span id=inside>open</span></template></div>
        <div id=closed><template shadowrootmode=closed><span>closed</span></template></div>
        <script>
          const open = document.getElementById('open');
          const root = open.shadowRoot;
          if (!(root instanceof ShadowRoot) || root.host !== open || root.mode !== 'open' ||
              root.querySelector('#inside').textContent !== 'open' ||
              document.querySelector('#inside')) throw Error('open root');
          const closed = document.getElementById('closed');
          if (closed.shadowRoot !== null) throw Error('closed root exposed');
          const reused = closed.attachShadow({mode:'closed'});
          if (!(reused instanceof ShadowRoot) || reused.host !== closed ||
              reused.mode !== 'closed' || reused.childNodes.length !== 0 ||
              closed.shadowRoot !== null) throw Error('declarative reuse');
          try { closed.attachShadow({mode:'closed'}); throw Error('duplicate accepted'); }
          catch (error) { if (error.name !== 'NotSupportedError') throw error; }
        </script>"#,
    );
}

#[test]
fn dom_parser_and_inner_html_leave_declarative_templates_inert() {
    check(
        r#"<body><script>
        const markup = '<template shadowrootmode=open><span>inert</span></template>';
        const host = document.createElement('div');
        host.innerHTML = markup;
        if (host.shadowRoot || host.firstChild.localName !== 'template' ||
            host.firstChild.content.firstChild.localName !== 'span') throw Error('innerHTML');
        const parsed = new DOMParser().parseFromString('<div>' + markup + '</div>', 'text/html');
        const detached = parsed.querySelector('div');
        if (detached.shadowRoot || detached.firstChild.localName !== 'template' ||
            detached.firstChild.content.firstChild.localName !== 'span') throw Error('DOMParser');
    </script>"#,
    );
}

#[test]
fn clonable_declarative_root_keeps_reuse_state_on_the_copy() {
    check(
        r#"<body>
        <div id=host><template shadowrootmode=open shadowrootclonable>
            <span>before</span></template></div>
        <script>
          const copy = document.getElementById('host').cloneNode(false);
          const root = copy.shadowRoot;
          if (!root || root.textContent.trim() !== 'before') throw Error('cloned contents');
          const liveChildren = root.childNodes;
          const observer = new MutationObserver(() => {});
          observer.observe(root, {childList:true});
          if (copy.attachShadow({mode:'open'}) !== root)
              throw Error('cloned declarative root identity');
          if (liveChildren.length || root.childNodes !== liveChildren)
              throw Error('cloned declarative root was not emptied');
          const records = observer.takeRecords();
          if (records.length !== 1 ||
              !Array.from(records[0].removedNodes).some(node => node.localName === 'span'))
              throw Error('reuse did not report removed nodes');
        </script>"#,
    );
}

#[test]
fn parsed_and_cloned_shadow_descendants_join_custom_element_traversal() {
    check(
        r#"<body>
        <div id=host><template shadowrootmode=open shadowrootclonable>
            <x-inside>text</x-inside></template></div>
        <script>
          let constructed = 0;
          customElements.define('x-inside', class extends HTMLElement {
              constructor() { super(); constructed++; }
          });
          const original = document.getElementById('host');
          if (!(original.shadowRoot.firstElementChild instanceof customElements.get('x-inside')))
              throw Error('parsed shadow descendant not upgraded');
          const copy = original.cloneNode(true);
          document.body.append(copy);
          if (!(copy.shadowRoot.firstElementChild instanceof customElements.get('x-inside')) ||
              constructed !== 2) throw Error('cloned shadow descendant not upgraded');
        </script>"#,
    );
}

#[test]
fn custom_registry_and_imperative_shadow_hosts_accept_dom_valid_local_names() {
    check(
        r#"<body><script>
          for (const name of ['x-😍', 'x-@', 'x-\u000b']) {
              class NamedElement extends HTMLElement {}
              customElements.define(name, NamedElement);
              if (customElements.get(name) !== NamedElement)
                  throw Error('registry rejected valid name: ' + name);
              const host = document.createElement(name);
              const root = host.attachShadow({mode:'open'});
              if (!(root instanceof ShadowRoot) || host.shadowRoot !== root)
                  throw Error('shadow host rejected valid name: ' + name);
          }
          for (const name of ['x/invalid', 'x-\0', 'x- ', 'x-Foo', 'font-face']) {
              try { customElements.define(name, class extends HTMLElement {}); }
              catch (error) {
                  if (error.name === 'SyntaxError') continue;
                  throw error;
              }
              throw Error('registry accepted invalid name: ' + name);
          }
          const reserved = document.createElement('font-face');
          try { reserved.attachShadow({mode:'open'}); }
          catch (error) {
              if (error.name !== 'NotSupportedError') throw error;
          }
          if (reserved.shadowRoot) throw Error('reserved host accepted');
        </script>"#,
    );
}
