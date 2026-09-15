use super::execute_html;

fn result(markup: &str, script: &str) -> String {
    let (dom, outcome) = execute_html(&format!(
        "{markup}<output id=result></output><script>{script}</script>"
    ));
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    dom.elements_named("output").next().unwrap().text_content()
}

#[test]
fn parsed_handler_keeps_its_listener_position_and_cancels_default() {
    assert_eq!(
        result(
            r#"<button id=b onclick="calls.push('markup'); return false">Go</button>"#,
            r#"const calls=[]; const b=document.getElementById('b');
        b.addEventListener('click', () => calls.push('listener'));
        let event=new Event('click', {cancelable:true});
        b.dispatchEvent(event);
        b.onclick=() => calls.push('property'); b.click();
        b.onclick=null; b.onclick=() => calls.push('last'); b.click();
        document.getElementById('result').textContent=calls.join(',')+'|'+event.defaultPrevented;"#
        ),
        "markup,listener,property,listener,listener,last|true"
    );
}

#[test]
fn content_attribute_mutations_replace_and_remove_lazy_handlers() {
    assert_eq!(
        result(
            "<button id=b></button>",
            r#"
        const b=document.getElementById('b'); const calls=[];
        b.setAttribute('ONCLICK', "calls.push('one')"); const old=b.onclick;
        b.setAttribute('onclick', "calls.push('one')"); const fresh=old!==b.onclick;
        b.click(); b.getAttributeNode('onclick').value="calls.push('two')"; b.click();
        b.setAttributeNS('urn:test', 'onclick', "calls.push('wrong')"); b.click();
        b.removeAttribute('onclick'); b.click();
        const a=document.createAttribute('onclick'); a.value="calls.push('three')";
        b.attributes.setNamedItem(a); b.click(); b.attributes.removeNamedItemNS(null,'onclick');
        b.click(); document.getElementById('result').textContent=calls.join(',')+'|'+fresh+'|'+b.onclick;
        "#
        ),
        "one,two,two,three|true|null"
    );
}

#[test]
fn compilation_uses_document_form_element_and_global_lexical_scopes() {
    assert_eq!(
        result(
            "<form id=f><button id=b></button></form>",
            r#"
        const b=document.getElementById('b'), f=document.getElementById('f');
        const lexical=7; document.docValue='doc'; f.formValue='form'; b.elementValue='element';
        b.setAttribute('onclick', "return [docValue,formValue,elementValue,lexical,this===event.target,typeof cache].join('|')");
        const event=new Event('click'); b.dispatchEvent(event);
        document.getElementById('result').textContent=b.onclick.call(b,event);
        "#
        ),
        "doc|form|element|7|true|undefined"
    );
}

#[test]
fn parser_body_load_handler_is_registered_before_window_listeners() {
    let (dom, outcome) = execute_html(
        r#"
        <body onload="document.getElementById('result').textContent += this===window?'body,':'wrong,'">
        <output id=result></output><script>
        window.addEventListener('load',()=>document.getElementById('result').textContent+='listener');
        </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "body,listener"
    );
}

#[test]
fn syntax_errors_compile_lazily_once_and_keep_the_listener_slot() {
    assert_eq!(
        result(
            "<button id=b></button>",
            r#"
        const b=document.getElementById('b'); const calls=[]; let errors=0;
        window.addEventListener('error', e=>{
          if(e.error instanceof SyntaxError && e.filename===document.URL) errors++;
          e.preventDefault();
        });
        b.setAttribute('onclick','}'); const lazy=errors===0;
        b.addEventListener('click',()=>calls.push('second'));
        const invalid=b.onclick===null && b.onclick===null;
        b.setAttribute('onclick',"calls.push('first')"); b.click();
        document.getElementById('result').textContent=[lazy,invalid,errors,calls.join(',')].join('|');
        "#
        ),
        "true|true|1|first,second"
    );
}

#[test]
fn inert_document_handlers_wait_for_adoption_and_clone_from_markup() {
    assert_eq!(
        result(
            "<div></div>",
            r#"
        const inert=document.implementation.createHTMLDocument('');
        const b=inert.createElement('button'); b.setAttribute('onclick','return 17');
        const disabled=b.onclick===null; document.adoptNode(b);
        const original=b.onclick; const clone=b.cloneNode();
        b.onclick=()=>99;
        document.getElementById('result').textContent=[disabled,original(),clone.onclick(),b.onclick()].join('|');
        "#
        ),
        "true|17|17|99"
    );
}

#[test]
fn element_scopes_shadow_outer_scopes_and_respect_unscopables() {
    assert_eq!(
        result(
            "<form id=f><button id=b></button></form>",
            r#"
        const b=document.getElementById('b'), f=document.getElementById('f');
        window.marker='window'; document.marker='document'; f.marker='form'; b.marker='button';
        Object.defineProperty(b,Symbol.unscopables,{value:{}});
        Object.defineProperty(f,Symbol.unscopables,{value:{}});
        b.setAttribute('onclick',"'use strict'; return marker");
        const values=[b.onclick()];
        b[Symbol.unscopables].marker=true; values.push(b.onclick());
        f[Symbol.unscopables].marker=true; values.push(b.onclick());
        document[Symbol.unscopables].marker=true; values.push(b.onclick());
        document.getElementById('result').textContent=values.join('|');
        "#
        ),
        "button|form|document|window"
    );
}

#[test]
fn body_error_handler_receives_five_arguments_and_only_true_cancels() {
    assert_eq!(
        result(
            "<body></body>",
            r#"
        window.details=[];
        document.body.setAttribute('onerror', "details.push([this===window,event,source,lineno,colno,error].join(',')); return true");
        const error=new ErrorEvent('error',{cancelable:true,message:'bad',filename:'file',lineno:2,colno:3,error:4});
        dispatchEvent(error);
        const e=document.createElement('img'); e.setAttribute('onerror','return false');
        const ordinary=new ErrorEvent('error',{cancelable:true}); e.dispatchEvent(ordinary);
        const alias=document.body.onerror===window.onerror;
        document.body.removeAttribute('onerror');
        document.getElementById('result').textContent=[details,alias,error.defaultPrevented,ordinary.defaultPrevented,window.onerror].join('|');
        "#
        ),
        "true,bad,file,2,3,4|true|true|true|"
    );
}

#[test]
fn inner_html_svg_and_custom_element_reactions_observe_current_handlers() {
    assert_eq!(
        result(
            "<div id=container></div>",
            r#"
        const container=document.getElementById('container'); window.calls=[];
        container.innerHTML="<button onclick='calls.push(1)'></button>";
        container.firstElementChild.click();
        container.insertAdjacentHTML('beforeend',"<button onclick='calls.push(2)'></button>");
        container.lastElementChild.click();
        const svg=document.createElementNS('http://www.w3.org/2000/svg','g');
        svg.setAttribute('onclick','calls.push(3)'); svg.dispatchEvent(new Event('click'));
        customElements.define('test-handler',class extends HTMLElement {
          static get observedAttributes(){return ['onclick'];}
          attributeChangedCallback(){calls.push(this.onclick());}
        });
        const custom=document.createElement('test-handler');
        custom.setAttribute('onclick','return 4');
        document.getElementById('result').textContent=calls.join(',');
        "#
        ),
        "1,2,3,4"
    );
}

#[test]
fn listener_exceptions_report_global_error_without_recursing_forever() {
    let (dom, outcome) = execute_html(
        r#"
        <button id=b onclick="throw new Error('handler failed')"></button><output></output>
        <script>
        let errors=0;
        window.onerror=()=>{errors++; throw new Error('error handler failed');};
        const b=document.getElementById('b'); let later=false;
        b.addEventListener('click',()=>later=true); b.click();
        document.querySelector('output').textContent=errors+'|'+later;
        </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "1|true"
    );
}

#[test]
fn beforeunload_coerces_returns_without_canceling_ordinary_events() {
    assert_eq!(
        result(
            "<body></body>",
            r#"
        document.body.setAttribute('onbeforeunload','return false');
        const ordinary=new Event('beforeunload',{cancelable:true});
        dispatchEvent(ordinary);
        const actual=document.createEvent('BeforeUnloadEvent');
        actual.initEvent('beforeunload',false,true); dispatchEvent(actual);
        window.onbeforeunload=null;
        document.getElementById('result').textContent=[ordinary.defaultPrevented,
          actual.defaultPrevented,actual.returnValue,actual instanceof BeforeUnloadEvent].join('|');
        "#
        ),
        "false|true|false|true"
    );
}
