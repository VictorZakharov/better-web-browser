use super::*;

fn check(body: &str) {
    let (_, outcome) = execute_html(&format!("<script>{body}</script>"));
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
}

#[test]
fn canvas_unsigned_long_reflection_uses_html_integer_prefix_and_default_range() {
    check(
        r#"
        const canvas=document.createElement('canvas');
        const assert=(ok,label)=>{if(!ok)throw Error(label);};
        for(const name of ['width','height']) {
            const fallback=name==='width'?300:150;
            for(const [raw,expected] of [['12px',12],[' +12.9',12],['\t\n17',17],
                ['01',1],['-1',fallback],['',fallback],['\u00a01',fallback],
                ['2147483647',2147483647],['2147483648',fallback],['999999999999999999999',fallback]]) {
                canvas.setAttribute(name,raw);assert(canvas[name]===expected,'attribute '+name+' '+raw);
            }
            for(const [value,expected] of [[2.9,2],[-0.9,0],[-1,fallback],
                [4294967296,0],[4294967297,1],[2147483648,fallback],[NaN,0],[Infinity,0]]) {
                canvas[name]=value;
                assert(canvas[name]===expected && canvas.getAttribute(name)===String(expected),'IDL '+name);
            }
            for(const value of [1n,Symbol()]) {
                let error='';try{canvas[name]=value;}catch(e){error=e.name;}
                assert(error==='TypeError','invalid numeric conversion');
            }
        }
        const descriptor=Object.getOwnPropertyDescriptor(HTMLCanvasElement.prototype,'width');
        let calls=0,error='';try{descriptor.set.call({}, {valueOf(){calls++;return 1;}});}catch(e){error=e.name;}
        assert(error==='TypeError' && calls===0,'brand before conversion');
        canvas.setAttribute=()=>{throw Error('author setAttribute');};
        canvas.width=4;assert(canvas.width===4,'private attribute setter');
        "#,
    );
}

#[test]
fn all_dimension_attribute_entry_points_reset_context_before_readback() {
    check(
        r#"
        const canvas=document.createElement('canvas');canvas.width=2;canvas.height=2;
        const ctx=canvas.getContext('2d'),assert=(ok,label)=>{if(!ok)throw Error(label);};
        const paint=()=>{ctx.fillStyle='red';ctx.fillRect(0,0,1,1);ctx.translate(1,1);};
        const cleared=label=>assert(ctx.fillStyle==='#000000' && ctx.getTransform().e===0 &&
            ctx.getImageData(0,0,1,1).data[3]===0,label);
        for(const mutation of [()=>canvas.setAttribute('width','2'),()=>canvas.setAttributeNS(null,'height','2'),
            ()=>{canvas.attributes.getNamedItem('width').value='2';},()=>{
                const attr=document.createAttribute('height');attr.value='2';canvas.setAttributeNode(attr);
            },()=>canvas.removeAttribute('width'),()=>canvas.removeAttributeNS(null,'height'),()=>{
                canvas.setAttribute('width','2');canvas.removeAttributeNode(canvas.getAttributeNode('width'));
            }]) {
            canvas.width=2;canvas.height=2;paint();mutation();cleared('dimension mutation');
        }
        canvas.width=2;canvas.height=2;paint();
        canvas.setAttributeNS('urn:other','width','1');
        assert(ctx.fillStyle==='#ff0000' && ctx.getTransform().e===1,
            'namespaced width is not a canvas dimension: '+[ctx.fillStyle,ctx.getTransform().e,canvas.width,canvas.height]);
        "#,
    );
}

#[test]
fn placeholder_dimensions_reject_attribute_sets_atomically_but_not_unrelated_namespaces() {
    check(
        r#"
        const canvas=document.createElement('canvas');canvas.width=2;canvas.height=3;
        canvas.transferControlToOffscreen();
        const assert=(ok,label)=>{if(!ok)throw Error(label);};
        for(const mutation of [()=>{canvas.width=4;},()=>canvas.setAttribute('width','4'),
            ()=>canvas.setAttributeNS(null,'height','4'),()=>{canvas.getAttributeNode('width').value='4';},
            ()=>{const a=document.createAttribute('height');a.value='4';canvas.attributes.setNamedItem(a);}]) {
            let error='';try{mutation();}catch(e){error=e.name;}
            assert(error==='InvalidStateError' && canvas.width===2 && canvas.height===3,'placeholder mutation');
        }
        canvas.setAttributeNS('urn:other','width','4');assert(canvas.width===2,'namespaced placeholder mutation');
        "#,
    );
}

#[test]
fn html_bitmap_renderer_keeps_transferred_bitmap_across_attribute_changes_until_null_transfer() {
    check(
        r#"
        const canvas=document.createElement('canvas');canvas.width=2;canvas.height=3;
        const renderer=canvas.getContext('bitmaprenderer');
        createImageBitmap(new ImageData(new Uint8ClampedArray([255,0,0,255]),1)).then(bitmap=>{
            renderer.transferFromImageBitmap(bitmap);canvas.width=4;canvas.setAttribute('height','5');
            return createImageBitmap(canvas);
        }).then(bitmap=>{
            if(bitmap.width!==1 || bitmap.height!==1)throw Error('attribute setter erased transferred bitmap');
            const target=new OffscreenCanvas(1,1).getContext('2d');target.drawImage(bitmap,0,0);
            if(String(target.getImageData(0,0,1,1).data)!=='255,0,0,255')throw Error('lost transferred pixels');
            renderer.transferFromImageBitmap(null);return createImageBitmap(canvas);
        }).then(bitmap=>{if(bitmap.width!==4 || bitmap.height!==5)throw Error('blank output did not resize');});
        "#,
    );
}
