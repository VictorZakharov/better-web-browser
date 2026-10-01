use super::check;

#[test]
fn input_subview_and_padded_stride_are_copied_without_aliasing() {
    check(
        r#"
        const buffer=new Uint8Array([99,99, 1,2,3,255,0,0,0,0, 4,5,6,255,0,0,0,0,99]);
        const frame=new VideoFrame(buffer.subarray(2,18),{format:'RGBA',codedWidth:1,codedHeight:2,
            timestamp:0,layout:[{offset:0,stride:8}]});
        buffer.fill(0);
        const output=new Uint8Array(8);
        frame.copyTo(output).then(()=>assert(output.join(',')==='1,2,3,255,4,5,6,255','source snapshot'));
    "#,
    );
}

#[test]
fn output_layout_leaves_padding_and_prefix_bytes_untouched() {
    check(
        r#"
        const frame=new VideoFrame(new Uint8Array([1,2,3,255,4,5,6,128]),
            {format:'RGBA',codedWidth:1,codedHeight:2,timestamp:0});
        const options={layout:[{offset:3,stride:7}]};
        assert(frame.allocationSize(options)===17,'allocation includes the last stride');
        const destination=new Uint8Array(20).fill(99);
        frame.copyTo(destination.subarray(2),options).then(layout=>{
            assert(destination.slice(0,5).every(x=>x===99),'prefix preserved');
            assert(destination.slice(5,9).join(',')==='1,2,3,255','first row');
            assert(destination.slice(9,12).every(x=>x===99),'padding preserved');
            assert(destination.slice(12,16).join(',')==='4,5,6,128','second row');
            assert(destination.slice(16).every(x=>x===99),'suffix preserved');
            assert(layout[0].stride===7,'returned actual stride');
        });
    "#,
    );
}

#[test]
fn invalid_buffer_dimensions_and_layouts_fail_before_allocation() {
    check(
        r#"
        const base={format:'RGBA',codedWidth:1,codedHeight:1,timestamp:0};
        for(const options of [
            {...base,codedWidth:0}, {...base,codedHeight:0}, {...base,codedWidth:-1},
            {...base,codedWidth:Infinity}, {...base,codedHeight:0x100000000},
            {...base,layout:[]}, {...base,layout:[{offset:0,stride:3}]},
            {...base,layout:[{offset:2,stride:4}]}, {...base,format:'INVALID'},
            {...base,displayWidth:1}, {...base,displayWidth:0,displayHeight:1}
        ]) {
            let failed=false;
            try{new VideoFrame(new Uint8Array(4),options);}catch(e){failed=true;}
            assert(failed,'invalid frame accepted '+JSON.stringify(options));
        }
    "#,
    );
}

#[test]
fn reordered_planar_layouts_are_valid_but_padding_ranges_cannot_overlap() {
    check(
        r#"
        const input=new Uint8Array(10);input.set([128,128],0);input.set([16,235,16,235],4);
        const frame=new VideoFrame(input,{format:'I420',codedWidth:2,codedHeight:2,timestamp:0,
            layout:[{offset:4,stride:2},{offset:0,stride:1},{offset:1,stride:1}]});
        const bytes=new Uint8Array(frame.allocationSize());
        frame.copyTo(bytes).then(()=>assert(bytes.join(',')==='16,235,16,235,128,128','reordered input planes'));
        let name;try{frame.allocationSize({layout:[{offset:0,stride:4},{offset:6,stride:1},{offset:9,stride:1}]});}
        catch(e){name=e.name;}
        assert(name==='TypeError','last luma padding belongs to its plane allocation');frame.close();
    "#,
    );
}

#[test]
fn detached_destination_rejects_and_does_not_close_the_frame() {
    check(
        r#"
        const frame=new VideoFrame(new Uint8Array([1,2,3,255]),{format:'RGBA',codedWidth:1,codedHeight:1,timestamp:0});
        const buffer=new ArrayBuffer(4);structuredClone(buffer,{transfer:[buffer]});
        frame.copyTo(buffer).then(()=>assert(false,'detached destination accepted'),error=>{
            assert(error.name==='TypeError'&&frame.codedWidth===1,'destination rejection retains source');frame.close();
        });
    "#,
    );
}

#[test]
fn admitted_copy_finishes_even_when_source_closes_before_promise_observation() {
    check(
        r#"
        const frame=new VideoFrame(new Uint8Array([4,5,6,255]),{format:'RGBA',codedWidth:1,codedHeight:1,timestamp:0});
        const destination=new Uint8Array(4),pending=frame.copyTo(destination);frame.close();
        pending.then(layout=>assert(destination.join(',')==='4,5,6,255'&&layout[0].stride===4,'accepted copy retains resource'));
        frame.copyTo(new Uint8Array(4)).then(()=>assert(false,'copy after close accepted'),error=>assert(error.name==='InvalidStateError','new copy sees close'));
    "#,
    );
}
