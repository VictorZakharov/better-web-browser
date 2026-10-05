use super::*;

fn check(code: &str) {
    let (_, outcome) = execute_html(&format!(
        "<body><canvas width=5 height=4></canvas><script>{code}</script>"
    ));
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
}

#[test]
fn canvas_image_data_row_copies_match_independent_clipping_oracle() {
    check(
        r#"
        const context = document.querySelector('canvas').getContext('2d');
        const source = new ImageData(4,3);
        for(let i=0;i<source.data.length;i++) source.data[i]=(i%4===3)?255:i+1;
        const cases = [
            [0,0,0,0,4,3],[-2,-1,0,0,4,3],[4,3,0,0,4,3],
            [1,1,3,2,-3,-2],[0,0,-2,-1,4,3],[1,0,2,0,5,3],
            [0,-2,0,1,4,2],[-1,0,1,0,3,3],[8,8,0,0,4,3],
            [0,0,0,0,0,3],[0,0,0,0,4,0],[0,0,9,9,-2,-2]
        ];
        for (const [dx,dy,x,y,width,height] of cases) {
            context.clearRect(0,0,5,4);
            context.putImageData(source,dx,dy,x,y,width,height);
            const expected = new Uint8ClampedArray(5*4*4);
            const lowX=Math.min(x,x+width), highX=Math.max(x,x+width);
            const lowY=Math.min(y,y+height), highY=Math.max(y,y+height);
            for(let sy=0;sy<3;sy++) for(let sx=0;sx<4;sx++) {
                if(sx<lowX||sx>=highX||sy<lowY||sy>=highY) continue;
                const tx=dx+sx,ty=dy+sy;
                if(tx<0||tx>=5||ty<0||ty>=4)continue;
                for(let c=0;c<4;c++)expected[(ty*5+tx)*4+c]=source.data[(sy*4+sx)*4+c];
            }
            const actual=context.getImageData(0,0,5,4).data;
            if(!actual.every((v,i)=>v===expected[i]))throw Error('write clipping '+[dx,dy,x,y,width,height]);
            for(const rect of [[-2,-1,8,6],[3,2,4,3],[2,2,-4,-3],[8,8,2,2]]) {
                let [rx,ry,rw,rh]=rect;
                const copy=context.getImageData(rx,ry,rw,rh);
                if(rw<0){rx+=rw;rw=-rw;}if(rh<0){ry+=rh;rh=-rh;}
                for(let row=0;row<rh;row++)for(let col=0;col<rw;col++)for(let c=0;c<4;c++) {
                    const px=rx+col,py=ry+row;
                    const want=px>=0&&px<5&&py>=0&&py<4?expected[(py*5+px)*4+c]:0;
                    if(copy.data[(row*rw+col)*4+c]!==want)throw Error('read clipping');
                }
            }
        }
    "#,
    );
}

#[test]
fn canvas_image_data_uses_private_storage_and_integer_conversion() {
    check(
        r#"
        const context=document.querySelector('canvas').getContext('2d');
        const image=new ImageData(new Uint8ClampedArray([10,20,30,255]),1,1);
        Object.defineProperty(image.data,'buffer',{get(){throw Error('author buffer getter');}});
        image.data.subarray=()=>{throw Error('author subarray');};
        context.putImageData(image,1.9,1.9);
        if(context.getImageData(1,1,1,1).data[0]!==10)throw Error('truncate');
        // Four to six arguments resolve to the three-argument overload.
        context.putImageData(image,2,1,Infinity);
        if(context.getImageData(2,1,1,1).data[0]!==10)throw Error('overload');
        const bad=[NaN,Infinity,-Infinity,2147483648,-2147483649,1n,Symbol()];
        for(const value of bad) {
            for(const call of [()=>context.putImageData(image,value,0),
                ()=>context.putImageData(image,0,0,0,0,value,1),
                ()=>context.getImageData(value,0,1,1)]) {
                let error;try{call();}catch(e){error=e;}
                if(!(error instanceof TypeError))throw Error('integer conversion '+String(value));
            }
        }
        let error;
        try{context.putImageData(Object.create(ImageData.prototype),0,0);}catch(e){error=e;}
        if(!(error instanceof TypeError))throw Error('forged image');
        error=undefined;
        try{context.putImageData.call({canvas:context.canvas},image,0,0);}catch(e){error=e;}
        if(!(error instanceof TypeError))throw Error('forged context');
        const other=new ImageData(1,1);
        structuredClone(other.data.buffer,{transfer:[other.data.buffer]});
        error=undefined;try{context.putImageData(other,0,0);}catch(e){error=e;}
        if(error?.name!=='InvalidStateError')throw Error('detached image');
    "#,
    );
}

#[test]
fn canvas_large_texture_read_write_remains_bounded_without_per_pixel_views() {
    check(
        r#"
        const canvas=document.querySelector('canvas');canvas.width=1024;canvas.height=1024;
        const context=canvas.getContext('2d');
        const image=new ImageData(1024,1024);
        image.data[0]=25;image.data[3]=255;image.data[image.data.length-4]=99;image.data[image.data.length-1]=255;
        for(let i=0;i<4;i++) {
            context.putImageData(image,0,0);
            const copy=context.getImageData(0,0,1024,1024);
            if(copy.data[0]!==25||copy.data[copy.data.length-4]!==99)throw Error('texture pixels');
        }
    "#,
    );
}
