use super::check;

#[test]
fn closing_one_reference_does_not_invalidate_a_clone() {
    check(
        r#"
        const frame=new VideoFrame(new Uint8Array([10,20,30,255]),{format:'RGBA',codedWidth:1,codedHeight:1,timestamp:9});
        const clone=frame.clone(); frame.close(); frame.close();
        assert(frame.format===null&&frame.codedWidth===0&&frame.visibleRect===null,'closed geometry');
        assert(clone.codedWidth===1&&clone.timestamp===9,'clone remains usable');
        let error;
        try{frame.clone();}catch(e){error=e.name;}
        assert(error==='InvalidStateError','closed clone rejected');
        const output=new Uint8Array(4);
        clone.copyTo(output).then(()=>assert(output.join(',')==='10,20,30,255','clone pixels'));
    "#,
    );
}
