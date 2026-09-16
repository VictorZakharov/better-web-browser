use super::*;

fn check_script(code: &str) {
    let (_, outcome) = execute_html(&format!("<!doctype html><body><script>{code}</script>"));
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
}

#[test]
fn html_parser_builds_an_inert_independent_document() {
    check_script(
        r#"
      const check=(ok,name)=>{if(!ok)throw Error(name)};
      const parser=new DOMParser();
      const parsed=parser.parseFromString('<!doctype html><title>Detached</title><p id="x">a &amp; b</p><noscript><b>enabled</b></noscript>','text/html');
      check(parsed!==document && parsed instanceof Document,'new document');
      check(parsed.defaultView===null && parsed.readyState==='complete','inactive document');
      document.cookie='active=yes'; parsed.cookie='detached=no';
      check(parsed.cookie==='' && !document.cookie.includes('detached=no'),'cookie-averse document');
      check(parsed.contentType==='text/html' && parsed.characterSet==='UTF-8','metadata');
      check(parsed.URL===document.URL && parsed.compatMode==='CSS1Compat','URL and mode');
      check(parsed.title==='Detached' && parsed.doctype.name==='html','tree');
      check(parsed.querySelector('noscript b').textContent==='enabled','scripting disabled');
      const p=parsed.getElementById('x');
      check(p.ownerDocument===parsed && p.textContent==='a & b','ownership');
      const copy=document.importNode(p,true);
      check(copy!==p && copy.ownerDocument===document && copy.textContent===p.textContent,'import');
      check(document.adoptNode(p)===p && p.ownerDocument===document && parsed.getElementById('x')===null,'adopt');
      document.body.append(p);
      const quirks=parser.parseFromString('<p>quirks','text/html');
      check(quirks.compatMode==='BackCompat','quirks');
      const clone=parsed.cloneNode(true);
      check(clone.contentType===parsed.contentType && clone.URL===parsed.URL,'clone metadata');
    "#,
    );
}

#[test]
fn parsed_scripts_stay_inert_and_custom_elements_upgrade_on_insertion() {
    check_script(
        r#"
      let upgrades=0; window.parsedScriptRuns=0;
      customElements.define('x-parsed', class extends HTMLElement {constructor(){super();upgrades++}});
      const parsed=new DOMParser().parseFromString('<x-parsed></x-parsed><script>window.parsedScriptRuns++</scr'+'ipt><template><script>window.parsedScriptRuns++</scr'+'ipt></template>','text/html');
      if(upgrades!==0 || window.parsedScriptRuns!==0)throw Error('not inert');
      const template=parsed.querySelector('template');
      document.body.append(...parsed.body.childNodes);
      document.body.append(document.importNode(template.content,true));
      if(upgrades!==1 || window.parsedScriptRuns!==0)throw Error('insertion lifecycle');
    "#,
    );
}

#[test]
fn xml_parser_preserves_namespaces_cdata_doctype_and_instructions() {
    check_script(
        r#"
      const check=(ok,name)=>{if(!ok)throw Error(name)};
      const source='<?xml version="1.0" encoding="utf-16"?><!DOCTYPE Root [<!ENTITY greeting "hello">]><?sample data?><Root xmlns="urn:r" xmlns:p="urn:p" p:value="A &amp; B"><p:Child>&greeting;<![CDATA[<raw>]]><!--comment--></p:Child></Root>';
      for(const type of ['text/xml','application/xml','application/xhtml+xml','image/svg+xml']) {
        const doc=new DOMParser().parseFromString(source,type), root=doc.documentElement;
        check(doc.contentType===type && doc.characterSet==='UTF-8' && doc.compatMode==='CSS1Compat','XML metadata');
        check(root.localName==='Root' && root.namespaceURI==='urn:r','root namespace');
        check(root.getAttributeNS('urn:p','value')==='A & B','attribute namespace');
        check(doc.doctype.name==='Root' && doc.childNodes[1].target==='sample' && doc.childNodes[1].data==='data','doctype and PI');
        const child=root.firstChild;
        check(child.prefix==='p' && child.nodeName==='p:Child' && child.ownerDocument===doc,'qualified child');
        check(child.childNodes[0].data==='hello','entity');
        const cdata=child.childNodes[1];
        check(cdata.nodeType===4 && cdata instanceof CDATASection && cdata instanceof Text && cdata.data==='<raw>','CDATA');
        check(cdata.cloneNode().nodeType===4 && document.importNode(cdata).data==='<raw>','CDATA cloning');
        cdata.data='new'; check(cdata.data==='new' && child.textContent==='hellonew','CDATA mutation');
        child.lastChild.data='changed'; check(child.lastChild.cloneNode().data==='changed','comment mutation');
        doc.childNodes[1].data='updated'; check(doc.childNodes[1].cloneNode().data==='updated','PI mutation');
      }
    "#,
    );
}

#[test]
fn xml_errors_are_documents_and_invalid_types_throw() {
    check_script(
        r#"
      const parser=new DOMParser();
      for(const source of ['', '<a>', '<a/><b/>','<p:unknown/>','<a x="1" x="2"/>','<a>&missing;</a>','<a xmlns:x="urn:s" xmlns:y="urn:s" x:n="1" y:n="2"/>']) {
        const doc=parser.parseFromString(source,'application/xml');
        if(doc.documentElement.localName!=='parsererror' || doc.documentElement.namespaceURI!=='http://www.mozilla.org/newlayout/xml/parsererror.xml')throw Error('accepted '+source);
      }
      for(const type of ['TEXT/HTML','text/html; charset=utf-8','text/plain',null,undefined,Symbol()]) {
        let error;try{parser.parseFromString('',type)}catch(e){error=e}
        if(!(error instanceof TypeError))throw Error('type accepted');
      }
      let error;try{parser.parseFromString('')}catch(e){error=e}
      if(!(error instanceof TypeError))throw Error('missing argument');
      error=null;try{parser.parseFromString.call({},'','text/html')}catch(e){error=e}
      if(!(error instanceof TypeError))throw Error('invalid receiver');
    "#,
    );
}

#[test]
fn xml_preserves_explicit_namespace_declarations_not_inherited_attributes() {
    check_script(
        r#"
      const doc=new DOMParser().parseFromString('<r xmlns="urn:r" xmlns:p="urn:p" xmlns:xml="http://www.w3.org/XML/1998/namespace">\r\n<p:c xmlns:p="urn:p"/><c/><c xmlns=""/></r>','text/xml');
      const root=doc.documentElement, [first,second,third]=root.children;
      if(root.attributes.length!==3 || first.attributes.length!==1 || second.attributes.length!==0 || third.attributes.length!==1)throw Error('namespace declaration identity '+[root,first,second,third].map(n=>n.getAttributeNames().join(',')));
      if(first.getAttribute('xmlns:p')!=='urn:p' || third.getAttribute('xmlns')!=='' || third.namespaceURI!==null)throw Error('namespace declaration values');
    "#,
    );
}

#[test]
fn detached_urls_are_snapshots_and_relative_links_follow_their_document_base() {
    check_script(
        r#"
      const parser = new DOMParser();
      const first = parser.parseFromString('<base href="/parsed/"><a href="child">link</a>', 'text/html');
      const original = document.URL;
      history.replaceState(null, '', '/changed/');
      const second = parser.parseFromString('<p>later', 'text/html');
      if (first.URL !== original || second.URL !== document.URL || first.URL === second.URL) throw Error('URL snapshot');
      if (first.baseURI !== 'https://example.com/parsed/' || first.querySelector('a').href !== 'https://example.com/parsed/child') throw Error('detached base URL');
      first.location = 'https://other.test/';
      if (first.location !== null || document.URL !== second.URL) throw Error('inactive location');
      for (const newline of ['\n', '\r', '\r\n']) {
        const xml = parser.parseFromString('<root xmlns:p="urn:p">'+newline+'<p:child xmlns:p="urn:p"/></root>', 'text/xml');
        if (xml.documentElement.firstElementChild.getAttribute('xmlns:p') !== 'urn:p') throw Error('newline namespace');
      }
    "#,
    );
}

#[test]
fn xml_depth_and_entity_recursion_fail_closed() {
    check_script(
        r#"
      for (const source of [
        '<r>'.repeat(513) + '</r>'.repeat(513),
        '<!DOCTYPE r [<!ENTITY cycle "&cycle;">]><r>&cycle;</r>'
      ]) {
        const parsed = new DOMParser().parseFromString(source, 'text/xml');
        if (parsed.documentElement.localName !== 'parsererror') throw Error('unbounded XML');
      }
    "#,
    );
}
