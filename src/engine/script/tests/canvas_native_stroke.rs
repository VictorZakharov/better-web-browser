use super::*;

#[test]
fn canvas_source_over_matches_independent_premultiplied_alpha_oracle() {
    let (_, outcome) = execute_html(
        r#"<canvas width=1 height=1></canvas><script>
        const c=document.querySelector('canvas').getContext('2d');
        const backdrop=new ImageData(1,1);
        for(const sa of [0,1,64,127,128,254,255])
        for(const da of [0,1,64,127,128,254,255])
        for(const opacity of [0,.25,.5,1]) {
            const src=[19,117,241,sa],dst=[229,83,7,da];
            backdrop.data.set(dst);c.putImageData(backdrop,0,0);
            c.fillStyle=`rgba(${src[0]},${src[1]},${src[2]},${sa/255})`;
            c.globalAlpha=opacity;c.fillRect(0,0,1,1);
            const a=sa/255*opacity,b=da/255,out=a+b*(1-a);
            const expected=src.slice(0,3).map((v,i)=>out===0?0:
                Math.round(255*(a*(v/255)+b*(1-a)*(dst[i]/255))/out));
            expected.push(Math.round(out*255));
            const actual=c.getImageData(0,0,1,1).data;
            if(expected.some((v,i)=>Math.abs(v-actual[i])>1))
                throw Error(`source-over ${sa}/${da}/${opacity}: ${actual} vs ${expected}`);
        }
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
}

#[test]
fn canvas_native_stroke_preserves_clipping_alpha_and_union_coverage() {
    let (_, outcome) = execute_html(
        r#"<canvas width=64 height=64></canvas><script>
        const c=document.querySelector('canvas').getContext('2d');
        c.rect(0,0,32,64);c.clip();c.beginPath();
        c.moveTo(8,32);c.lineTo(56,32);c.moveTo(8,32);c.lineTo(56,32);
        c.lineWidth=8;c.strokeStyle='#ff0000';c.globalAlpha=.5;c.stroke();
        const pixel=(x,y)=>Array.from(c.getImageData(x,y,1,1).data).join(',');
        if(pixel(16,32)!=='255,0,0,128')throw Error('overlapping strokes darkened twice');
        if(pixel(40,32)!=='0,0,0,0')throw Error('native stroke ignored clip');
        if(pixel(16,20)!=='0,0,0,0')throw Error('native stroke exceeded its width');
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
}

#[test]
fn canvas_native_stroke_handles_repeated_large_closed_paths_with_real_pixels() {
    let (_, outcome) = execute_html(
        r#"<canvas width=512 height=512></canvas><script>
        const c=document.querySelector('canvas').getContext('2d');
        c.strokeStyle='#00ff00';c.lineWidth=12;c.lineJoin='round';
        for(let j=0;j<8;j++) {
            c.beginPath();
            for(let i=0;i<128;i++) {
                const a=i*Math.PI/64,x=256+180*Math.cos(a),y=256+180*Math.sin(a);
                if(i===0)c.moveTo(x,y);else c.lineTo(x,y);
            }
            c.closePath();c.stroke();
        }
        if(c.getImageData(435,256,1,1).data[1]!==255)throw Error('missing stroke');
        if(c.getImageData(256,256,1,1).data[3]!==0)throw Error('stroke filled interior');
    </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
}
