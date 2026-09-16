//! Native parser records must be observable before a synchronous write returns.
use super::parser_writes::run;
#[path = "parser_custom_elements.rs"]
mod custom_elements;

#[test]
fn parser_records_preserve_siblings_text_old_values_and_observer_identity() {
    let report = run(r#"<!doctype html><body><script id=writer>
      const observer=new MutationObserver(()=>{throw Error('unexpected delivery')});
      observer.observe(document,{subtree:true,childList:true,characterData:true,characterDataOldValue:true});
      observer.observe(document.body,{subtree:true,childList:true});
      document.write('<p id=p>A');
      document.write('B</p><!--tail-->');
      const records=observer.takeRecords().map(r=>[r.type,r.target.nodeName,
        [...r.addedNodes].map(n=>n.nodeName),r.previousSibling?.nodeName||null,
        r.nextSibling?.nodeName||null,r.oldValue]);
      observer.disconnect();
      document.title=JSON.stringify(records);
    </script>"#);
    assert_eq!(
        report["titles"]["document_title"],
        r##"[["childList","BODY",["P"],"SCRIPT",null,null],["childList","P",["#text"],null,null,null],["characterData","#text",[],null,null,"A"],["childList","BODY",["#comment"],"P",null,null]]"##,
        "{report}"
    );
}

#[test]
fn parser_records_are_delivered_at_the_checkpoint_before_the_next_script() {
    let report = run(r#"<!doctype html><body><script>
      window.order=[];
      const observer=new MutationObserver(records=>{
        if(records.some(r=>[...r.addedNodes].some(n=>n.id==='parsed'))) order.push('observer');
      });
      observer.observe(document.body,{childList:true});
    </script><p id=parsed>parsed</p><script>
      order.push('script'); observer.disconnect(); document.title=JSON.stringify(order);
    </script>"#);
    assert_eq!(
        report["titles"]["document_title"], r#"["observer","script"]"#,
        "{report}"
    );
}

#[test]
fn written_records_do_not_run_callbacks_inside_author_script() {
    let report = run(r#"<!doctype html><body><script>
      window.order=[];
      const observer=new MutationObserver(()=>{order.push('observer'); observer.disconnect()});
      observer.observe(document.body,{childList:true});
      document.write('<b>written</b><script>order.push("nested");<\/script>');
      order.push('returned');
      Promise.resolve().then(()=>{order.push('promise'); document.title=JSON.stringify(order)});
    </script>"#);
    assert_eq!(
        report["titles"]["document_title"], r#"["nested","returned","observer","promise"]"#,
        "{report}"
    );
}

#[test]
fn script_created_streams_keep_observing_the_replaced_document() {
    let report = run(r#"<!doctype html><body><script>
      window.onload=()=>{
        const observer=new MutationObserver(()=>{});
        observer.observe(document,{childList:true,subtree:true});
        document.open(); observer.takeRecords();
        document.write('<!doctype html><title>replacement</title><body><p>text</p>');
        const records=observer.takeRecords();
        observer.disconnect(); document.close();
        document.title=JSON.stringify([records.some(r=>r.target===document&&r.addedNodes[0]?.nodeName==='HTML'),
          records.some(r=>r.target.nodeName==='P'&&r.addedNodes[0]?.data==='text')]);
      };
    </script>"#);
    assert_eq!(
        report["titles"]["document_title"], "[true,true]",
        "{report}"
    );
}
