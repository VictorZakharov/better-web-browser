use super::*;

fn check(source: &str) {
    let wire = include_str!("../bootstrap/webgl_private_wire.js");
    let script = format!("(()=>{{{wire}\n{source}}})();");
    let (_, outcome) = execute_html(&format!("<script>{script}</script>"));
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    let (runtime, outcome) = WorkerRuntime::start(
        "https://example.test/webgl-private-wire.js",
        &format!("{script}postMessage('passed');"),
        "",
        ScriptKind::Classic,
        std::sync::Arc::new(|url, _| Err(format!("unexpected {url}"))),
    );
    assert!(runtime.is_some());
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.messages, ["\"passed\""]);
}

#[test]
fn native_command_values_do_not_consult_page_json_tojson_or_array_helpers() {
    check(
        r#"
        const stringify=JSON.stringify,parse=JSON.parse,map=Array.prototype.map,every=Array.prototype.every;
        const finite=Number.isFinite,integer=Number.isSafeInteger,nan=Number.isNaN,same=Object.is;
        const expected=stringify({op:'probe',i:[0,2147483648,4294967295],f:['-0','nan','inf','-inf',1.25],text:'quote" Ω\n'});
        let observed=0,command,options,list;
        const fail=()=>{observed++;throw Error('author hook')};
        Object.defineProperty(Object.prototype,'toJSON',{configurable:true,get:fail});
        JSON.stringify=JSON.parse=Array.prototype.map=Array.prototype.every=fail;
        Number.isFinite=Number.isSafeInteger=Number.isNaN=Object.is=fail;
        try {
            command=webGlWireCommand('probe',[0,2147483648,4294967295],[-0,NaN,Infinity,-Infinity,1.25],'quote" Ω\n');
            options=webGlWireOptions({alpha:true,api:'webgl2',power_preference:'default'});
            list=webGlWireList(['u',0,4294967295]);
            if(webGlWireCommand('bad',[.5])!==null || webGlWireCommand('bad',[],['wrong'])!==null)
                throw Error('validation changed');
        } finally {
            delete Object.prototype.toJSON;JSON.stringify=stringify;JSON.parse=parse;
            Array.prototype.map=map;Array.prototype.every=every;
            Number.isFinite=finite;Number.isSafeInteger=integer;Number.isNaN=nan;Object.is=same;
        }
        if(observed || command!==expected || options!==stringify({alpha:true,api:'webgl2',power_preference:'default'}) ||
            list!=='["u",0,4294967295]')throw Error('private wire used author hooks');
    "#,
    );
}

#[test]
fn native_replies_preserve_special_floats_without_mutable_parser_or_inherited_loss() {
    check(
        r#"
        const parse=JSON.parse,includes=String.prototype.includes,keys=Object.keys;
        let observed=0,result;
        const fail=()=>{observed++;throw Error('native reply exposed')};
        Object.defineProperty(Object.prototype,'lost',{configurable:true,get:fail});
        JSON.parse=String.prototype.includes=Object.keys=fail;
        try {
            result=webGlWireParse('[{"webglFloat":"-0"},{"webglFloat":"nan"},{"webglFloat":"inf"},{"webglFloat":"-inf"},42]');
            if(webGlWireLost(result) || webGlWireLost(12) || webGlWireLost(null) || !webGlWireLost(webGlWireParse('{"lost":true}')))
                throw Error('loss detection');
            const nested=webGlWireParse('{"record":{"webglFloat":"inf","other":1}}');
            if(nested.record.other!==1 || nested.record.webglFloat!=='inf')throw Error('sentinel shape changed');
        } finally {delete Object.prototype.lost;JSON.parse=parse;String.prototype.includes=includes;Object.keys=keys;}
        if(observed || !Object.is(result[0],-0) || !Number.isNaN(result[1]) || result[2]!==Infinity || result[3]!==-Infinity || result[4]!==42)
            throw Error('special float reply conversion');
    "#,
    );
}

#[test]
fn unexpected_nested_objects_cannot_run_tojson_in_the_primitive_wire() {
    check(
        r#"
        let observed=0;
        const value={toJSON(){observed++;return 1}};
        for(const call of [()=>webGlWireList([value]),()=>webGlWireOptions({alpha:value}),
            ()=>webGlWireCommand('probe',[],[],value)]) {
            let rejected=false;try{call()}catch(error){rejected=error instanceof TypeError}
            if(!rejected || observed)throw Error('nested author object serialized');
        }
    "#,
    );
}

#[test]
fn real_window_and_worker_webgl_continue_with_poisoned_json_hooks() {
    let source = r#"
        const gl=make(8,4).getContext('webgl2');if(!gl)throw Error('context missing');
        const stringify=JSON.stringify,parse=JSON.parse;
        let calls=0,pixels=new Uint8Array(4);
        const fail=()=>{calls++;throw Error('author JSON hook')};
        Object.defineProperty(Object.prototype,'toJSON',{configurable:true,get:fail});
        JSON.stringify=JSON.parse=fail;
        try {
            const buffer=gl.createBuffer();gl.bindBuffer(gl.ARRAY_BUFFER,buffer);
            gl.bufferData(gl.ARRAY_BUFFER,new Float32Array([1,2,3,4]),gl.STATIC_DRAW);
            if(gl.getBufferParameter(gl.ARRAY_BUFFER,gl.BUFFER_SIZE)!==16)throw Error('buffer reply');
            gl.clearColor(1,0,0,1);gl.clear(gl.COLOR_BUFFER_BIT);
            gl.readPixels(0,0,1,1,gl.RGBA,gl.UNSIGNED_BYTE,pixels);
            if(gl.getError()!==gl.NO_ERROR)throw Error('native error');
            gl.deleteBuffer(buffer);
        } finally {delete Object.prototype.toJSON;JSON.stringify=stringify;JSON.parse=parse;}
        if(calls || pixels[0]!==255 || pixels[1]!==0 || pixels[2]!==0 || pixels[3]!==255)
            throw Error('page hooks changed native command results');
    "#;
    let (_, outcome) = execute_html(&format!(
        "<script>const make=(w,h)=>{{const c=document.createElement('canvas');c.width=w;c.height=h;return c;}};{source}</script>"
    ));
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    let (runtime, outcome) = WorkerRuntime::start(
        "https://example.test/real-webgl-private-wire.js",
        &format!("const make=(w,h)=>new OffscreenCanvas(w,h);{source};postMessage('passed');"),
        "",
        ScriptKind::Classic,
        std::sync::Arc::new(|url, _| Err(format!("unexpected {url}"))),
    );
    assert!(runtime.is_some());
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.messages, ["\"passed\""]);
}
