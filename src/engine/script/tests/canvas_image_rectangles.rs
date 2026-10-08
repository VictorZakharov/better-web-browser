use super::*;

const SOURCE: &str = include_str!("../../../../tests/canvas/image-data-rectangles.js");

#[test]
fn image_data_readback_does_not_consult_an_author_replaced_data_getter() {
    let source = r#"
        const context=make(7,5).getContext('2d');
        const image=new ImageData(7,5);
        for(let i=0;i<image.data.length;i++)image.data[i]=i%4===3?255:i%256;
        context.putImageData(image,0,0);
        const descriptor=Object.getOwnPropertyDescriptor(ImageData.prototype,'data');
        let calls=0,result;
        Object.defineProperty(ImageData.prototype,'data',{configurable:true,get(){calls++;throw Error('author data getter')}});
        try {result=context.getImageData(0,-1,7,7)} finally {
            Object.defineProperty(ImageData.prototype,'data',descriptor);
        }
        if(calls || result.width!==7 || result.height!==7)throw Error('author getter called internally');
        for(let i=0;i<image.data.length;i++)if(result.data[7*4+i]!==image.data[i])throw Error('private copied pixels');
        for(let i=0;i<7*4;i++)if(result.data[i] || result.data[result.data.length-1-i])throw Error('padding');
    "#;
    let (_, outcome) = execute_html(&format!(
        "<script>const make=(w,h)=>{{const c=document.createElement('canvas');c.width=w;c.height=h;return c;}};{source}</script>"
    ));
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    let (runtime, outcome) = WorkerRuntime::start(
        "https://example.test/image-data-private-read.js",
        &format!("const make=(w,h)=>new OffscreenCanvas(w,h);{source};postMessage('passed');"),
        "",
        ScriptKind::Classic,
        std::sync::Arc::new(|url, _| Err(format!("unexpected {url}"))),
    );
    assert!(runtime.is_some());
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.messages, ["\"passed\""]);
}

#[test]
fn image_data_rectangle_pixels_match_an_independent_oracle_in_both_realms() {
    let (_, outcome) = execute_html(&format!(
        "<script>{SOURCE};if(testImageDataRectangles((w,h)=>{{const c=document.createElement('canvas');c.width=w;c.height=h;return c;}})!==630)throw Error('assertion count');</script>"
    ));
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    let (runtime, outcome) = WorkerRuntime::start(
        "https://example.test/image-data-rectangles.js",
        &format!(
            "{SOURCE};if(testImageDataRectangles((w,h)=>new OffscreenCanvas(w,h))!==630)throw Error('assertion count');postMessage('passed');"
        ),
        "",
        ScriptKind::Classic,
        std::sync::Arc::new(|url, _| Err(format!("unexpected {url}"))),
    );
    assert!(runtime.is_some());
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.messages, ["\"passed\""]);
}
