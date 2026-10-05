use super::*;

#[test]
fn affine_pen_matches_independent_inverse_rectangle_oracle() {
    let (_, outcome) = execute_html(
        r#"<canvas width=96 height=96></canvas><script>
        const c=document.querySelector('canvas').getContext('2d');
        const matrices=[[2,0,0,3,10,5],[0,2,-3,0,80,10],
            [1,.5,.75,1,15,10],[-2,0,0,2,80,5],[.5,0,0,.75,20,20]];
        for(const m of matrices) for(const external of [false,true]) {
            c.resetTransform();c.clearRect(0,0,96,96);c.setTransform(...m);
            c.lineWidth=6;c.lineCap='butt';c.strokeStyle='red';
            const path=external?new Path2D():c;
            if(!external)c.beginPath();path.moveTo(5,15);path.lineTo(25,15);
            external?c.stroke(path):c.stroke();
            const [a,b,k,d,e,f]=m,det=a*d-b*k;
            const data=c.getImageData(0,0,96,96).data;
            for(let y=0;y<96;y++)for(let x=0;x<96;x++) {
                const dx=x+.5-e,dy=y+.5-f;
                const u=(d*dx-k*dy)/det,v=(-b*dx+a*dy)/det;
                // Exclude edge-adjacent samples: binary rasterizers differ in
                // tie handling, but must agree throughout the interior/exterior.
                if(Math.min(Math.abs(u-5),Math.abs(u-25),Math.abs(v-12),Math.abs(v-18))<.2)continue;
                const inside=u>5&&u<25&&v>12&&v<18;
                if((data[(y*96+x)*4+3]!==0)!==inside)
                    throw Error(`affine pen ${m} / ${external} at ${x},${y}`);
                if(c.isPointInStroke(...(external?[path,x+.5,y+.5]:[x+.5,y+.5]))!==inside)
                    throw Error(`affine hit test ${m} / ${external} at ${x},${y}`);
            }
        }
        </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
}

#[test]
fn default_path_keeps_construction_geometry_but_uses_paint_time_pen() {
    let (_, outcome) = execute_html(
        r#"<canvas width=80 height=80></canvas><script>
        const c=document.querySelector('canvas').getContext('2d');
        c.beginPath();c.moveTo(10,30);c.lineTo(60,30);
        c.scale(2,3);c.lineWidth=4;c.stroke();
        const alpha=(x,y)=>c.getImageData(x,y,1,1).data[3];
        if(alpha(20,25)!==255||alpha(20,23)!==0||alpha(65,30)!==0)
            throw Error('painting CTM moved current path or failed to scale pen');
        c.resetTransform();c.clearRect(0,0,80,80);c.beginPath();
        c.scale(2,3);c.moveTo(5,10);c.lineTo(30,10);c.resetTransform();c.stroke();
        if(alpha(20,29)!==255||alpha(20,26)!==0||alpha(65,30)!==0)
            throw Error('construction CTM was reapplied to pen');
        </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
}

#[test]
fn transformed_round_caps_are_ellipses_and_singular_pens_paint_nothing() {
    let (_, outcome) = execute_html(
        r#"<canvas width=80 height=80></canvas><script>
        const c=document.querySelector('canvas').getContext('2d');
        c.scale(3,1);c.beginPath();c.moveTo(10,30);c.lineTo(15,30);
        c.lineWidth=10;c.lineCap='round';c.stroke();
        const alpha=(x,y)=>c.getImageData(x,y,1,1).data[3];
        if(alpha(18,30)!==255||alpha(30,24)!==0||alpha(14,30)!==0)
            throw Error('round cap did not become an elliptical pen');
        c.resetTransform();c.clearRect(0,0,80,80);
        c.setTransform(1,0,0,0,0,0);c.stroke();
        if(c.getImageData(0,0,80,80).data.some(v=>v!==0)||c.isPointInStroke(30,30))
            throw Error('singular pen painted or hit');
        c.resetTransform();c.stroke();
        if(alpha(30,30)!==255)throw Error('singular paint destroyed current path');
        </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
}

#[test]
fn transformed_dashed_fallback_scales_dash_lengths_and_preserves_path2d() {
    let (_, outcome) = execute_html(
        r#"<canvas width=80 height=60></canvas><script>
        const c=document.querySelector('canvas').getContext('2d');
        const path=new Path2D();path.moveTo(5,10);path.lineTo(35,10);
        c.scale(2,3);c.lineWidth=4;c.setLineDash([4,4]);c.stroke(path);
        const alpha=(x,y)=>c.getImageData(x,y,1,1).data[3];
        if(alpha(12,25)!==255||alpha(20,30)!==0||alpha(28,35)!==255)
            throw Error('dash geometry was not traced in painting coordinates');
        c.resetTransform();c.clearRect(0,0,80,60);c.setLineDash([]);c.stroke(path);
        if(alpha(10,10)!==255||alpha(40,10)!==0)
            throw Error('stroke mutated supplied Path2D');
        </script>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
}
