//! Native presentation must not turn tainted backing pixels into author callbacks.
use super::*;

fn input(node: &NodeRef, code: &str) -> ScriptInput {
    ScriptInput {
        source_url: "https://example.test/#presentation-privacy".into(),
        code: code.into(),
        node: node.clone(),
        kind: ScriptKind::Classic,
        fetch_options: ScriptFetchOptions::for_kind(ScriptKind::Classic),
        finish_lifecycle: false,
    }
}

#[test]
fn canvas_presentation_private_pixels_cannot_escape_through_array_push() {
    let dom =
        dom::parse_with_scripting("<canvas width=2 height=2></canvas><script></script>", true);
    let script = dom.elements_named("script").next().unwrap();
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.test/");
    let outcome = runtime.execute_initial(&[input(
        &script,
        r#"
        const canvas=document.querySelector('canvas'),context=canvas.getContext('2d');
        context.fillStyle='#112233';context.filter='drop-shadow(0 0 currentColor)';
        context.fillRect(0,0,2,2);
        let denied=false;
        try{context.getImageData(0,0,1,1);}catch(error){denied=error.name==='SecurityError';}
        if(!denied)throw Error('fixture did not taint its source');
        const originalPush=Array.prototype.push;
        globalThis.presentationLeakBytes=0;
        Array.prototype.push=function(...args){
            const record=args[0];
            if(Array.isArray(record) && record.length===6 && ArrayBuffer.isView(record[5]))
                presentationLeakBytes+=record[5].byteLength;
            return originalPush.apply(this,args);
        };
    "#,
    )]);
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    let snapshots = runtime.take_canvas_presentation().unwrap();
    assert_eq!(snapshots.len(), 1);
    assert_eq!(
        snapshots[0].pixels.as_deref().unwrap(),
        [17, 34, 51, 255].repeat(4)
    );
    let outcome = runtime.execute_additional_with_loader(&[input(&script, r#"
        Array.prototype.push=originalPush;
        if(presentationLeakBytes!==0)throw Error('private presentation pixels reached author code: '+presentationLeakBytes);
    "#)], None);
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
}

#[test]
fn canvas_presentation_uses_private_collection_and_iteration_intrinsics() {
    let dom =
        dom::parse_with_scripting("<canvas width=1 height=1></canvas><script></script>", true);
    let script = dom.elements_named("script").next().unwrap();
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.test/");
    let outcome = runtime.execute_initial(&[input(&script, r#"
        const canvas=document.querySelector('canvas'),context=canvas.getContext('2d');
        context.fillStyle='#123456';context.fillRect(0,0,1,1);
        const originalDescriptors=[];
        const originalApply=Reflect.apply;
        let collectionPixelLeaks=0;
        function inspect(value){
            if(value && ArrayBuffer.isView(value.pixels)) collectionPixelLeaks++;
            if(Array.isArray(value) && value.length===6 && ArrayBuffer.isView(value[5]))
                collectionPixelLeaks++;
        }
        function replace(object,key){
            const descriptor=Object.getOwnPropertyDescriptor(object,key);
            originalDescriptors[originalDescriptors.length]=[object,key,descriptor];
            Object.defineProperty(object,key,descriptor.get
                ? {configurable:true,get(){inspect(this);return originalApply(descriptor.get,this,[]);}}
                : {...descriptor,value(...args){
                    inspect(this);
                    for(let i=0;i<args.length;i++)inspect(args[i]);
                    const result=originalApply(descriptor.value,this,args);inspect(result);return result;
                }});
        }
        replace(Array.prototype,'sort');replace(Array.prototype,Symbol.iterator);
        replace(Set.prototype,Symbol.iterator);replace(Set.prototype,'forEach');
        replace(Set.prototype,'size');replace(Set.prototype,'values');
        replace(Set.prototype,'add');replace(Set.prototype,'delete');replace(Set.prototype,'has');
        replace(WeakMap.prototype,'get');replace(WeakMap.prototype,'set');replace(WeakMap.prototype,'delete');
        replace(WeakSet.prototype,'add');replace(WeakSet.prototype,'delete');replace(WeakSet.prototype,'has');
        replace(WeakRef.prototype,'deref');
    "#)]);
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    let snapshots = runtime.take_canvas_presentation().unwrap();
    assert_eq!(snapshots.len(), 1);
    assert_eq!(
        snapshots[0].pixels.as_deref(),
        Some([18, 52, 86, 255].as_slice())
    );
    assert!(runtime.take_canvas_presentation().unwrap().is_empty());
    let outcome = runtime.execute_additional_with_loader(
        &[input(
            &script,
            r#"
        for(let i=0;i<originalDescriptors.length;i++){
            const record=originalDescriptors[i];
            Object.defineProperty(record[0],record[1],record[2]);
        }
        if(collectionPixelLeaks!==0)throw Error('private bitmap reached collection callbacks');
        canvas.remove();
    "#,
        )],
        None,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert!(runtime.take_canvas_presentation().unwrap().is_empty());
    let outcome = runtime.execute_additional_with_loader(
        &[input(
            &script,
            r#"
        document.body.appendChild(canvas);
    "#,
        )],
        None,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(runtime.take_canvas_presentation().unwrap().len(), 1);
}
