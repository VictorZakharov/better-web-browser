//! Bitmap renderer consumes image ownership instead of implicitly resampling it.
use super::*;

fn check(body: &str, expected: &str) {
    let html = format!("<body><output>pending</output><script>{body}</script></body>");
    let (dom, outcome) = execute_html(&html);
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        expected
    );
}

#[test]
fn transferred_bitmap_natural_size_does_not_mutate_canvas_content_attributes() {
    check(
        r#"
        const canvas=document.createElement('canvas');canvas.width=10;canvas.height=8;
        const renderer=canvas.getContext('bitmaprenderer');
        const source=new ImageData(2,1);source.data.set([255,0,0,255,0,0,255,255]);
        createImageBitmap(source).then(bitmap=>{
            renderer.transferFromImageBitmap(bitmap);
            return createImageBitmap(canvas);
        }).then(bitmap=>{
            const target=new OffscreenCanvas(2,1).getContext('2d');target.drawImage(bitmap,0,0);
            document.querySelector('output').textContent=[canvas.width,canvas.height,bitmap.width,bitmap.height,
                [...target.getImageData(0,0,2,1).data].join('/')].join(',');
        });
    "#,
        "10,8,2,1,255/0/0/255/0/0/255/255",
    );
}

#[test]
fn null_transfer_restores_attribute_sized_blank_bitmap_after_a_different_sized_image() {
    check(
        r#"
        const canvas=new OffscreenCanvas(4,3),renderer=canvas.getContext('bitmaprenderer');
        createImageBitmap(new ImageData(2,1)).then(bitmap=>{
            renderer.transferFromImageBitmap(bitmap);renderer.transferFromImageBitmap(null);
            return createImageBitmap(canvas);
        }).then(bitmap=>{
            const target=new OffscreenCanvas(4,3).getContext('2d');target.drawImage(bitmap,0,0);
            document.querySelector('output').textContent=[bitmap.width,bitmap.height,
                [...target.getImageData(0,0,4,3).data].every(value=>value===0)].join(',');
        });
    "#,
        "4,3,true",
    );
}

#[test]
fn opaque_renderer_composites_source_over_black_without_changing_source_clones() {
    check(
        r#"
        const source=new ImageData(2,1);source.data.set([255,128,64,128,250,100,50,0]);
        createImageBitmap(source,{premultiplyAlpha:'none'}).then(bitmap=>{
            const clone=structuredClone(bitmap);
            const canvas=new OffscreenCanvas(5,5),renderer=canvas.getContext('bitmaprenderer',{alpha:false});
            renderer.transferFromImageBitmap(bitmap);
            const target=new OffscreenCanvas(2,1).getContext('2d');target.drawImage(canvas,0,0);
            const opaque=[...target.getImageData(0,0,2,1).data].join(',');
            target.clearRect(0,0,2,1);target.drawImage(clone,0,0);
            const original=[...target.getImageData(0,0,2,1).data].join(',');
            document.querySelector('output').textContent=opaque+'|'+original;
        });
    "#,
        "128,64,32,255,0,0,0,255|255,128,64,128,0,0,0,0",
    );
}

#[test]
fn opaque_blank_initial_reset_and_null_bitmap_are_black_not_transparent() {
    check(
        r#"
        const canvas=new OffscreenCanvas(1,1),renderer=canvas.getContext('bitmaprenderer',{alpha:false});
        const sample=()=>{
            const target=new OffscreenCanvas(1,1).getContext('2d');target.drawImage(canvas,0,0);
            return [...target.getImageData(0,0,1,1).data].join(',');
        };
        const initial=sample();canvas.width=1;const reset=sample();renderer.transferFromImageBitmap(null);
        document.querySelector('output').textContent=[initial,reset,sample()].join('|');
    "#,
        "0,0,0,255|0,0,0,255|0,0,0,255",
    );
}

#[test]
fn repeated_get_context_does_not_reread_settings_or_change_alpha_policy() {
    check(
        r#"
        const canvas=new OffscreenCanvas(1,1),calls=[];
        const settings={get alpha(){calls.push('alpha');return false;}};
        const first=canvas.getContext('bitmaprenderer',settings);
        const second=canvas.getContext('bitmaprenderer',{get alpha(){throw Error('second getter');}});
        document.querySelector('output').textContent=[first===second,calls.join(',')].join('|');
    "#,
        "true|alpha",
    );
}

#[test]
fn failed_context_dictionary_conversion_does_not_lock_canvas_context_mode() {
    check(
        r#"
        const canvas=new OffscreenCanvas(1,1);let error='';
        try{canvas.getContext('bitmaprenderer',{get alpha(){throw Error('marker');}});}
        catch(value){error=value.message;}
        document.querySelector('output').textContent=[error,canvas.getContext('2d') instanceof
            OffscreenCanvasRenderingContext2D].join(',');
    "#,
        "marker,true",
    );
}

#[test]
fn renderer_members_and_transfer_enforce_receiver_and_argument_brands() {
    check(
        r#"
        const descriptor=Object.getOwnPropertyDescriptor(ImageBitmapRenderingContext.prototype,'canvas');
        const failures=[];
        for(const receiver of [{},null,Object.create(ImageBitmapRenderingContext.prototype)]) {
            try{descriptor.get.call(receiver);failures.push(false);}catch(error){failures.push(error instanceof TypeError);}
            try{ImageBitmapRenderingContext.prototype.transferFromImageBitmap.call(receiver,null);failures.push(false);}
            catch(error){failures.push(error instanceof TypeError);}
        }
        const renderer=new OffscreenCanvas(1,1).getContext('bitmaprenderer');
        for(const bitmap of [undefined,{},Object.create(ImageBitmap.prototype)]) {
            try{renderer.transferFromImageBitmap(bitmap);failures.push(false);}catch(error){failures.push(error instanceof TypeError);}
        }
        document.querySelector('output').textContent=failures.every(Boolean);
    "#,
        "true",
    );
}

#[test]
fn consumed_image_cannot_be_transferred_to_a_second_renderer() {
    check(
        r#"
        createImageBitmap(new ImageData(1,1)).then(bitmap=>{
            const first=new OffscreenCanvas(1,1).getContext('bitmaprenderer');
            const second=new OffscreenCanvas(1,1).getContext('bitmaprenderer');
            first.transferFromImageBitmap(bitmap);let error='';
            try{second.transferFromImageBitmap(bitmap);}catch(value){error=value.name;}
            document.querySelector('output').textContent=[bitmap.width,error].join(',');
        });
    "#,
        "0,InvalidStateError",
    );
}

#[test]
fn offscreen_active_renderer_rejects_canvas_transfer_without_detaching_context() {
    check(
        r#"
        const canvas=new OffscreenCanvas(1,1),renderer=canvas.getContext('bitmaprenderer');
        let error='';
        try{structuredClone(canvas,{transfer:[canvas]});}catch(value){error=value.name;}
        renderer.transferFromImageBitmap(null);
        document.querySelector('output').textContent=[renderer.canvas===canvas,error,canvas.width].join(',');
    "#,
        "true,InvalidStateError,1",
    );
}

#[test]
fn context_identifiers_are_case_sensitive_and_keep_the_web_idl_function_length() {
    check(
        r#"
        const html=document.createElement('canvas'),offscreen=new OffscreenCanvas(1,1);
        let invalid='';try{offscreen.getContext('2D');}catch(error){invalid=error.name;}
        const checks=[html.getContext('BITMAPRENDERER')===null,invalid==='TypeError',
            HTMLCanvasElement.prototype.getContext.length===1,OffscreenCanvas.prototype.getContext.length===1];
        document.querySelector('output').textContent=checks.every(Boolean);
    "#,
        "true",
    );
}

#[test]
fn offscreen_renderer_rejected_transfer_preserves_attribute_size_natural_size_and_opaque_policy() {
    check(
        r#"
        const canvas=new OffscreenCanvas(4,3),renderer=canvas.getContext('bitmaprenderer',{alpha:false});
        createImageBitmap(new ImageData(2,1)).then(bitmap=>{
            renderer.transferFromImageBitmap(bitmap);
            let error='';try{structuredClone(canvas,{transfer:[canvas]});}catch(value){error=value.name;}
            if(error!=='InvalidStateError')throw new Error('active renderer was transferred');
            return createImageBitmap(canvas).then(image=>({moved:canvas,image}));
        }).then(({moved,image})=>{
            const dimensions=[moved.width,moved.height,image.width,image.height];
            moved.getContext('bitmaprenderer').transferFromImageBitmap(null);
            const target=new OffscreenCanvas(4,3).getContext('2d');target.drawImage(moved,0,0);
            document.querySelector('output').textContent=dimensions.join(',')+'|'+
                [...target.getImageData(0,0,1,1).data].join(',');
        });
    "#,
        "4,3,2,1|0,0,0,255",
    );
}
