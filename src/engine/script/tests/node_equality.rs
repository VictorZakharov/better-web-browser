use super::*;

fn check(code: &str) {
    let (_, outcome) = execute_html(&format!(
        r#"<!doctype html><body><script>
        const check=(ok,label)=>{{if(!ok)throw Error(label)}};
        {code}</script>"#
    ));
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
}

#[test]
fn node_equality_compares_native_light_tree_data_without_attribute_order() {
    check(
        r#"
        const a=document.createElement('section'), b=document.createElement('section');
        a.setAttribute('x','1');a.setAttribute('y','2');
        b.setAttribute('y','2');b.setAttribute('x','1');
        a.innerHTML='<b>text</b><!--comment-->';b.innerHTML=a.innerHTML;
        check(a.isEqualNode(b) && b.isEqualNode(a),'equal trees');
        check(a.isSameNode(a) && !a.isSameNode(b),'identity');
        b.firstChild.firstChild.data='changed';check(!a.isEqualNode(b),'text differs');
        b.innerHTML=a.innerHTML;b.append(b.firstChild);check(!a.isEqualNode(b),'ordered children');
        b.innerHTML=a.innerHTML;b.setAttribute('x','3');check(!a.isEqualNode(b),'attribute differs');
        check(!a.isEqualNode(null) && !a.isEqualNode() && !a.isSameNode(undefined),'nullable');
        for (const method of ['isEqualNode','isSameNode']) {
            for (const other of [0,{},Object.create(Node.prototype)]) {
                let failed=false;try {a[method](other)}catch(e){failed=e instanceof TypeError}
                check(failed,'invalid argument '+method);
            }
            let failed=false;try {Node.prototype[method].call({},null)}catch(e){failed=e instanceof TypeError}
            check(failed,'invalid receiver '+method);
        }
        const copy=a.cloneNode(true);
        for(const key of ['nodeType','nodeName','childNodes','attributes','namespaceURI','localName'])
            Object.defineProperty(a,key,{get(){throw Error('author getter '+key)}});
        check(Node.prototype.isEqualNode.call(a,copy),'native equality');
    "#,
    );
}

#[test]
fn node_equality_compares_namespaces_node_kinds_and_attribute_values() {
    check(
        r#"
        const xml=new DOMParser().parseFromString('<r/>','application/xml');
        const a=xml.createElementNS('urn:example','a:x'), b=xml.createElementNS('urn:example','b:x');
        check(!a.isEqualNode(b),'element prefixes matter');
        const first=xml.createAttributeNS('urn:example','a:x'), second=xml.createAttributeNS('urn:example','b:x');
        first.value=second.value='same';check(first.isEqualNode(second),'attribute prefixes ignored');
        a.setAttributeNodeNS(first);b.setAttributeNodeNS(second);
        Object.defineProperty(first,'value',{get(){throw Error('author value getter')}});
        check(first.isEqualNode(second),'attached private state');
        a.setAttributeNS('urn:example','a:x','changed');check(!first.isEqualNode(second),'attached native value');
        const parse=source=>new DOMParser().parseFromString(source,'application/xml');
        const cdata=parse('<r><![CDATA[x]]></r>').documentElement.firstChild;
        check(!xml.createTextNode('x').isEqualNode(cdata),'different interfaces');
        check(cdata.isEqualNode(parse('<r><![CDATA[x]]></r>').documentElement.firstChild),'CDATA');
        check(xml.createComment('x').isEqualNode(xml.createComment('x')),'comment');
        check(!xml.createComment('x').isEqualNode(xml.createComment('y')),'comment value');
        const pi=parse('<?t x?><r/>').firstChild;
        check(pi.isEqualNode(parse('<?t x?><r/>').firstChild),'PI');
        check(!pi.isEqualNode(parse('<?u x?><r/>').firstChild),'PI target');
        const doctype=parse('<!DOCTYPE r PUBLIC "p" "s"><r/>').doctype;
        check(doctype.isEqualNode(parse('<!DOCTYPE r PUBLIC "p" "s"><r/>').doctype),'doctype');
        check(!doctype.isEqualNode(parse('<!DOCTYPE r PUBLIC "q" "s"><r/>').doctype),'public id');
        check(!doctype.isEqualNode(parse('<!DOCTYPE r PUBLIC "p" "t"><r/>').doctype),'system id');
        check(!document.isEqualNode(document.createDocumentFragment()),'document vs fragment');
    "#,
    );
}

#[test]
fn node_equality_ignores_template_and_shadow_contents_and_handles_foreign_realms() {
    check(
        r#"
        const a=document.createElement('template'), b=document.createElement('template');
        a.innerHTML='<b>A</b>';b.innerHTML='<p>B</p>';check(a.isEqualNode(b),'template contents');
        const host=document.createElement('div'), other=document.createElement('div');
        host.attachShadow({mode:'open'}).innerHTML='<span>shadow</span>';
        check(host.isEqualNode(other),'shadow contents');
        const frame=document.createElement('iframe');document.body.append(frame);
        const foreign=frame.contentDocument;
        const one=document.createElement('meta'), two=foreign.createElement('meta');
        one.setAttribute('name','description');two.setAttribute('name','description');
        check(one.isEqualNode(two) && two.isEqualNode(one),'cross realm nodes');
        const attr=one.getAttributeNode('name'), remote=two.getAttributeNode('name');
        check(attr.isEqualNode(remote) && remote.isEqualNode(attr),'cross realm attrs');
        check(Node.prototype.isSameNode.call(remote,remote),'cross realm identity');
        check(!attr.isEqualNode(one),'attribute vs element');
    "#,
    );
}
