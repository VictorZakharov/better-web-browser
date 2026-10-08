//! Public ImageBitmap resize retains ownership, precision and worker parity.
use super::*;
use std::sync::Arc;

const BODY: &str = r#"
    const source = new ImageData(new Uint8ClampedArray([255,0,0,255,0,0,255,0]),2,1);
    const pending = createImageBitmap(source,{resizeWidth:3,resizeHeight:1,resizeQuality:'low',premultiplyAlpha:'none'});
    source.data.fill(0);
    pending.then(bitmap => {
        const canvas = new OffscreenCanvas(3,1), context = canvas.getContext('2d');
        context.imageSmoothingEnabled=false;
        context.drawImage(bitmap,0,0);
        const actual=Array.from(context.getImageData(0,0,3,1).data).join(',');
        if(actual!=='255,0,0,255,255,0,0,128,0,0,0,0')throw Error('resize pixels '+actual);
        bitmap.close();
        if(bitmap.width!==0 || bitmap.height!==0)throw Error('close did not detach');
        RESULT('pass');
    });
"#;

#[test]
fn bitmap_native_resize_owns_its_snapshot_and_preserves_transparent_edges() {
    let body = BODY.replace(
        "RESULT('pass')",
        "document.querySelector('output').textContent='pass'",
    );
    let (dom, outcome) = execute_html(&format!("<output>pending</output><script>{body}</script>"));
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "pass"
    );
}

#[test]
fn bitmap_native_resize_worker_uses_the_same_sampler_and_ownership_contract() {
    let body = BODY.replace("RESULT('pass')", "postMessage('pass')");
    let (_, outcome) = WorkerRuntime::start(
        "https://example.com/resize-worker.js",
        &body,
        "",
        ScriptKind::Classic,
        Arc::new(|_, _| Err("unexpected import".into())),
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.messages, vec!["\"pass\""]);
}

#[test]
fn bitmap_native_resize_does_not_hand_private_samples_to_replaceable_pixel_methods() {
    let (dom, outcome) = execute_html(
        r#"
        <output>pending</output><script>
        const source=new ImageData(new Uint8ClampedArray([17,31,47,255,197,113,29,128]),2,1);
        const original=Object.getPrototypeOf(Uint8Array.prototype).set;
        const descriptor=Object.getOwnPropertyDescriptor(Object.getPrototypeOf(Uint8Array.prototype),'buffer');
        const prototype=Object.getPrototypeOf(Uint8Array.prototype);
        let escaped=0;
        prototype.set=function(...args){escaped++;return original.apply(this,args);};
        Object.defineProperty(prototype,'buffer',{configurable:true,get(){escaped++;return descriptor.get.call(this);}});
        createImageBitmap(source,{resizeWidth:7,resizeHeight:3,premultiplyAlpha:'none'}).then(bitmap=>{
            prototype.set=original;Object.defineProperty(prototype,'buffer',descriptor);
            const canvas=new OffscreenCanvas(7,3),context=canvas.getContext('2d');
            context.imageSmoothingEnabled=false;context.drawImage(bitmap,0,0);
            const pixels=context.getImageData(0,0,7,3).data;
            if(escaped!==0 || pixels[0]!==17 || pixels[3]!==255)throw Error('private resize storage escaped');
            bitmap.close();document.querySelector('output').textContent='pass';
        });
        </script>
    "#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "pass"
    );
}
