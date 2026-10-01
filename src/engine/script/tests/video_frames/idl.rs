use super::check;

#[test]
fn dictionary_callbacks_cannot_use_closed_resources_or_skip_webidl_conversion() {
    check(
        r#"
        const make=()=>new VideoFrame(new Uint8Array(4),{format:'RGBA',codedWidth:1,codedHeight:1,timestamp:0});
        const allocation=make();let name;
        try{allocation.allocationSize({get format(){allocation.close();return 'RGBA';}});}catch(e){name=e.name;}
        assert(name==='InvalidStateError','option callback closes resource before algorithm');
        const copy=make();
        copy.copyTo(new Uint8Array(4),{get format(){copy.close();return 'RGBA';}}).then(
            ()=>assert(false,'closed during conversion'),e=>assert(e.name==='InvalidStateError','copy observes closed state'));
        const closed=make();closed.close();let visited=false;
        try{closed.allocationSize({get format(){visited=true;return Symbol();}});}catch(e){name=e.name;}
        assert(visited&&name==='TypeError','IDL conversion precedes closed-state check');
    "#,
    );
}

#[test]
fn copy_dictionary_reads_and_converts_each_member_exactly_once() {
    check(
        r#"
        const frame=new VideoFrame(new Uint8Array([1,2,3,255]),{format:'RGBA',codedWidth:1,codedHeight:1,timestamp:0});
        const visits=[];
        const options={get colorSpace(){visits.push('color');return 'srgb';},
            get format(){visits.push('format');return {toString(){visits.push('convert');return 'BGRA';}};},
            get layout(){visits.push('layout');return undefined;},get rect(){visits.push('rect');return undefined;}};
        assert(frame.allocationSize(options)===4,'allocation');
        assert(visits.join(',')==='color,format,convert,layout,rect','single ordered dictionary access '+visits);
        visits.length=0;const pixels=new Uint8Array(4);
        frame.copyTo(pixels,options).then(()=>{
            assert(visits.join(',')==='color,format,convert,layout,rect','copy dictionary order');
            assert(pixels.join(',')==='3,2,1,255','converted actual pixels');frame.close();
        });
    "#,
    );
}

#[test]
fn color_dictionary_converts_a_member_before_reading_the_next() {
    check(
        r#"
        const visits=[];
        const color=new VideoColorSpace({get fullRange(){visits.push('range');return false;},
            get matrix(){visits.push('matrix');return {toString(){visits.push('matrix-value');return 'bt709';}};},
            get primaries(){visits.push('primaries');return 'bt709';},
            get transfer(){visits.push('transfer');return 'bt709';}});
        assert(visits.join(',')==='range,matrix,matrix-value,primaries,transfer','color IDL order '+visits);
        assert(color.matrix==='bt709'&&color.fullRange===false,'converted metadata');
        let later=false,rejected=false;
        try{new VideoColorSpace({matrix:Symbol(),get primaries(){later=true;}});}catch(e){rejected=e instanceof TypeError;}
        assert(rejected&&!later,'failed conversion stops later getters');
    "#,
    );
}

#[test]
fn buffer_dictionary_getters_are_visited_once_in_webidl_order() {
    check(
        r#"
        const visited=[];
        const values={codedHeight:1,codedWidth:1,colorSpace:undefined,displayHeight:undefined,
            displayWidth:undefined,duration:undefined,flip:undefined,format:'RGBA',layout:undefined,
            metadata:undefined,rotation:undefined,timestamp:0,transfer:undefined,visibleRect:undefined};
        const options={};
        for(const name of Object.keys(values))Object.defineProperty(options,name,{get(){visited.push(name);return values[name];}});
        Object.defineProperty(options,'alpha',{get(){throw Error('alpha is not in VideoFrameBufferInit');}});
        const frame=new VideoFrame(new Uint8Array(4),options);
        assert(visited.join(',')===Object.keys(values).join(','),'buffer dictionary conversion order '+visited);frame.close();
    "#,
    );
}

#[test]
fn constructor_numeric_members_truncate_but_do_not_wrap_enforce_range() {
    check(
        r#"
        const frame=new VideoFrame(new Uint8Array(4),{format:'RGBA',codedWidth:1.9,codedHeight:1.8,timestamp:-2.9,duration:3.9});
        assert(frame.codedWidth===1&&frame.codedHeight===1&&frame.timestamp===-2&&frame.duration===3,'integer truncation');frame.close();
        for(const timestamp of [NaN,Infinity,-Infinity,2**63,-(2**63)-2048,1n]){
            let rejected=false;
            try{new VideoFrame(new Uint8Array(4),{format:'RGBA',codedWidth:1,codedHeight:1,timestamp});}
            catch(e){rejected=e instanceof TypeError;}
            assert(rejected,'EnforceRange timestamp '+String(timestamp));
        }
    "#,
    );
}

#[test]
fn canvas_init_uses_non_enforce_range_long_long_conversion() {
    check(
        r#"
        const canvas=new OffscreenCanvas(1,1);canvas.getContext('2d');
        for(const timestamp of [Infinity,NaN,2**64]){
            const frame=new VideoFrame(canvas,{timestamp});
            assert(frame.timestamp===0,'IDL non-EnforceRange time');frame.close();
        }
    "#,
    );
}

#[test]
fn every_public_getter_and_method_checks_its_receiver_brand() {
    check(
        r#"
        for(const member of ['format','codedWidth','codedHeight','codedRect','visibleRect','displayWidth','displayHeight',
            'timestamp','duration','rotation','flip','colorSpace']){
            let rejected=false;
            try{Object.getOwnPropertyDescriptor(VideoFrame.prototype,member).get.call({});}
            catch(e){rejected=e instanceof TypeError;}
            assert(rejected,'getter brand '+member);
        }
        for(const member of ['allocationSize','clone','close','metadata']){
            let rejected=false;try{VideoFrame.prototype[member].call({});}catch(e){rejected=e instanceof TypeError;}
            assert(rejected,'method brand '+member);
        }
        VideoFrame.prototype.copyTo.call({},new Uint8Array(4)).then(()=>assert(false,'copy brand'),
            error=>assert(error instanceof TypeError,'copy brand rejection'));
    "#,
    );
}

#[test]
fn dictionary_symbols_and_bigints_do_not_silently_stringify_or_round() {
    check(
        r#"
        const base={format:'RGBA',codedWidth:1,codedHeight:1,timestamp:0};
        for(const options of [{...base,format:Symbol('RGBA')},{...base,codedWidth:1n},
            {...base,rotation:1n},{...base,duration:1n},{...base,visibleRect:{x:0n,y:0,width:1,height:1}}]){
            let rejected=false;try{new VideoFrame(new Uint8Array(4),options);}catch(e){rejected=e instanceof TypeError;}
            assert(rejected,'symbol/BigInt conversion');
        }
    "#,
    );
}

#[test]
fn unsupported_valid_pixel_enums_reject_with_not_supported() {
    check(
        r#"
        for(const format of ['I420P10','I420P12','I422P10','I422P12','I444P10','I444P12',
            'I420AP10','I420AP12','I422AP10','I422AP12','I444AP10','I444AP12','RGBAF16']){
            let name;try{new VideoFrame(new Uint8Array(8),{format,codedWidth:1,codedHeight:1,timestamp:0});}
            catch(error){name=error.name;}
            assert(name==='NotSupportedError','honest unsupported format '+format);
        }
    "#,
    );
}

#[test]
fn duplicate_frame_transfer_validation_does_not_short_circuit_dictionary_getters() {
    check(
        r#"
        const bytes=new Uint8Array(4),visits=[];
        let name;try{new VideoFrame(bytes,{format:'RGBA',codedWidth:1,codedHeight:1,timestamp:0,
            transfer:[bytes.buffer,bytes.buffer],get visibleRect(){visits.push('visible');return undefined;}});}
        catch(e){name=e.name;}
        assert(name==='DataCloneError'&&visits.join(',')==='visible'&&bytes.byteLength===4,'dictionary first, no partial detach');
    "#,
    );
}
