use super::run;

#[test]
fn network_constructor_and_reaction_jobs_run_before_parser_resumption() {
    let report = run(r#"<!doctype html><body><script>
      window.order=[];
      customElements.define('checkpoint-test',class extends HTMLElement {
        static observedAttributes=['id','data-value'];
        constructor(){super();order.push('construct');
          queueMicrotask(()=>order.push(['construct job',this.id,this.isConnected]));}
        attributeChangedCallback(name){order.push(name);
          queueMicrotask(()=>order.push([name+' job',this.isConnected,this.childNodes.length]));}
        connectedCallback(){order.push('connect');
          queueMicrotask(()=>order.push(['connect job',this.childNodes.length]));}
      });
    </script><checkpoint-test id=one data-value=ok><b>child</b></checkpoint-test><script>
      order.push('script');document.title=JSON.stringify(order);
    </script>"#);
    assert_eq!(
        report["titles"]["document_title"],
        r#"["construct",["construct job","",false],"id",["id job",false,0],"data-value",["data-value job",false,0],"connect",["connect job",0],"script"]"#,
        "{report}"
    );
}

#[test]
fn written_custom_elements_construct_before_attributes_connection_and_children() {
    let report = run(r#"<!doctype html><body><script>
      window.order=[];
      customElements.define('parser-test',class extends HTMLElement {
        static observedAttributes=['data-value','id'];
        constructor(){super();order.push(['constructor',this.attributes.length,this.isConnected,this.childNodes.length]);}
        attributeChangedCallback(name,old,value){order.push([name,old,value,this.id,this.isConnected,this.childNodes.length]);}
        connectedCallback(){order.push(['connected',this.id,this.childNodes.length]);}
      });
      document.write('<parser-test data-value=ok id=one><b>child</b></parser-test>');
      order.push(['returned',document.getElementById('one').textContent]);
      document.title=JSON.stringify(order);
    </script>"#);
    assert_eq!(
        report["titles"]["document_title"],
        r#"[["constructor",0,false,0],["data-value",null,"ok","one",false,0],["id",null,"one","one",false,0],["connected","one",0],["returned","child"]]"#,
        "{report}"
    );
}

#[test]
fn network_custom_elements_run_before_their_child_tokens() {
    let report = run(r#"<!doctype html><body><script>
      window.order=[];
      customElements.define('network-test',class extends HTMLElement {
        constructor(){super(); order.push(['constructor',this.id,this.parentNode===null]);}
        connectedCallback(){order.push(['connected',this.id,this.childNodes.length]);}
      });
    </script><network-test id=one><network-test id=two>child</network-test></network-test><script>
      document.title=JSON.stringify(order);
    </script>"#);
    assert_eq!(
        report["titles"]["document_title"],
        r#"[["constructor","",true],["connected","one",0],["constructor","",true],["connected","two",0]]"#,
        "{report}"
    );
}

#[test]
fn parser_attribute_reactions_reject_dynamic_markup_and_templates_stay_inert() {
    let report = run(r#"<!doctype html><body><script>
      window.order=[];
      customElements.define('guard-test',class extends HTMLElement {
        static observedAttributes=['data-value'];
        constructor(){super();order.push('constructor');}
        attributeChangedCallback(){try{document.write('<i>wrong</i>')}catch(e){order.push(e.name)}}
        connectedCallback(){order.push('connected');}
      });
      document.write('<template id=t><guard-test data-value=x></guard-test></template>');
      order.push(document.getElementById('t').content.firstChild.getAttribute('data-value'));
      document.write('<guard-test data-value=x></guard-test>');
      document.title=JSON.stringify(order);
    </script>"#);
    assert_eq!(
        report["titles"]["document_title"],
        r#"["x","constructor","InvalidStateError","connected"]"#,
        "{report}"
    );
}

#[test]
fn a_failed_parser_constructor_inserts_a_distinct_unknown_element() {
    let report = run(r#"<!doctype html><body><script>
      window.leaked=null; window.errors=[];
      window.onerror=(m,s,l,c,e)=>{errors.push(e.name);return true};
      customElements.define('failing-test',class extends HTMLElement {
        constructor(){super();leaked=this;this.setAttribute('bad','value');}
      });
      document.write('<failing-test id=fallback><b>child</b></failing-test>');
      const fallback=document.getElementById('fallback');
      document.title=JSON.stringify([errors,fallback instanceof HTMLUnknownElement,
        fallback!==leaked,fallback.hasAttribute('bad'),fallback.textContent,leaked.parentNode===null]);
    </script>"#);
    assert_eq!(
        report["titles"]["document_title"],
        r#"[["NotSupportedError"],true,true,false,"child",true]"#,
        "{report}"
    );
}

#[test]
fn connected_callback_writes_use_the_current_parser_insertion_point() {
    let report = run(r#"<!doctype html><body><script>
      customElements.define('writer-test',class extends HTMLElement {
        connectedCallback(){document.write('<i>callback</i>');}
      });
      document.write('<writer-test id=one><b>source</b></writer-test>');
      document.title=document.getElementById('one').innerHTML;
    </script>"#);
    assert_eq!(
        report["titles"]["document_title"], "<i>callback</i><b>source</b>",
        "{report}"
    );
}
