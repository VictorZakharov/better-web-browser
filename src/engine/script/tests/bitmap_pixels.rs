//! Pixel-level ImageBitmap ownership, alpha and geometric transformations.
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
fn image_data_is_snapshotted_before_promise_jobs_run() {
    check(
        r#"
        const data=new ImageData(2,1);
        data.data.set([255,0,0,255,0,0,255,255]);
        const pending=createImageBitmap(data);
        data.data.fill(0);
        pending.then(bitmap=>{
            const target=new OffscreenCanvas(2,1).getContext('2d');
            target.drawImage(bitmap,0,0);
            document.querySelector('output').textContent=[...target.getImageData(0,0,2,1).data].join(',');
        });
    "#,
        "255,0,0,255,0,0,255,255",
    );
}

#[test]
fn canvas_snapshot_survives_source_reset_and_bitmap_closing_is_independent() {
    check(
        r#"
        const source=new OffscreenCanvas(1,1),paint=source.getContext('2d');
        paint.fillStyle='red';paint.fillRect(0,0,1,1);
        const pending=createImageBitmap(source);
        source.width=0;
        pending.then(bitmap=>{
            const copy=structuredClone(bitmap), moved=structuredClone(bitmap,{transfer:[bitmap]});
            copy.close();
            const target=new OffscreenCanvas(1,1).getContext('2d');target.drawImage(moved,0,0);
            document.querySelector('output').textContent=[bitmap.width,copy.width,moved.width,
                [...target.getImageData(0,0,1,1).data].join('/')].join(',');
        });
    "#,
        "0,0,1,255/0/0/255",
    );
}

#[test]
fn negative_crop_extents_normalize_rectangle_without_flipping_pixels() {
    check(
        r#"
        const source=new ImageData(3,2);
        [1,2,3,4,5,6].forEach((red,index)=>source.data.set([red,0,0,255],index*4));
        createImageBitmap(source,3,2,-2,-2,{premultiplyAlpha:'none'}).then(bitmap=>{
            const target=new OffscreenCanvas(2,2).getContext('2d');target.drawImage(bitmap,0,0);
            document.querySelector('output').textContent=[...target.getImageData(0,0,2,2).data]
                .filter((_,index)=>index%4===0).join(',');
        });
    "#,
        "2,3,5,6",
    );
}

#[test]
fn outside_crop_samples_are_transparent_black_and_do_not_wrap_into_source_rows() {
    check(
        r#"
        const source=new ImageData(1,1);source.data.set([255,0,0,255]);
        createImageBitmap(source,-1,-1,3,3).then(bitmap=>{
            const target=new OffscreenCanvas(3,3).getContext('2d');target.drawImage(bitmap,0,0);
            const data=[...target.getImageData(0,0,3,3).data];
            document.querySelector('output').textContent=data.every((value,index)=>
                value===(index===16||index===19?255:0));
        });
    "#,
        "true",
    );
}

#[test]
fn flip_y_happens_after_crop_and_preserves_horizontal_order() {
    check(
        r#"
        const source=new ImageData(3,2);
        [1,2,3,4,5,6].forEach((red,index)=>source.data.set([red,0,0,255],index*4));
        createImageBitmap(source,1,0,2,2,{imageOrientation:'flipY'}).then(bitmap=>{
            const target=new OffscreenCanvas(2,2).getContext('2d');target.drawImage(bitmap,0,0);
            document.querySelector('output').textContent=[...target.getImageData(0,0,2,2).data]
                .filter((_,index)=>index%4===0).join(',');
        });
    "#,
        "5,6,2,3",
    );
}

#[test]
fn straight_and_explicit_premultiplied_alpha_are_real_distinct_storage_policies() {
    check(
        r#"
        const source=new ImageData(2,1);source.data.set([101,51,25,128,250,100,50,0]);
        Promise.all(['none','premultiply'].map(premultiplyAlpha=>createImageBitmap(source,{premultiplyAlpha})))
            .then(bitmaps=>{
                const values=bitmaps.map(bitmap=>{
                    const target=new OffscreenCanvas(2,1).getContext('2d');target.drawImage(bitmap,0,0);
                    return [...target.getImageData(0,0,2,1).data].join(',');
                });
                document.querySelector('output').textContent=values.join('|');
            });
    "#,
        "101,51,25,128,0,0,0,0|102,52,26,128,0,0,0,0",
    );
}

#[test]
fn clone_and_transfer_preserve_premultiplication_metadata_without_double_multiplication() {
    check(
        r#"
        const source=new ImageData(1,1);source.data.set([255,128,64,128]);
        createImageBitmap(source,{premultiplyAlpha:'premultiply'}).then(bitmap=>{
            const clone=structuredClone(bitmap), moved=structuredClone(bitmap,{transfer:[bitmap]});
            const target=new OffscreenCanvas(2,1).getContext('2d');
            target.drawImage(clone,0,0);target.drawImage(moved,1,0);
            document.querySelector('output').textContent=[...target.getImageData(0,0,2,1).data].join(',');
        });
    "#,
        "255,128,64,128,255,128,64,128",
    );
}

#[test]
fn creating_a_bitmap_from_premultiplied_bitmap_first_recovers_straight_samples() {
    check(
        r#"
        const source=new ImageData(1,1);source.data.set([255,128,64,128]);
        createImageBitmap(source,{premultiplyAlpha:'premultiply'})
            .then(bitmap=>createImageBitmap(bitmap,{premultiplyAlpha:'premultiply'}))
            .then(bitmap=>{
                const target=new OffscreenCanvas(1,1).getContext('2d');target.drawImage(bitmap,0,0);
                document.querySelector('output').textContent=[...target.getImageData(0,0,1,1).data].join(',');
            });
    "#,
        "255,128,64,128",
    );
}

#[test]
fn integer_pixelated_upscale_duplicates_exact_source_pixels() {
    check(
        r#"
        const source=new ImageData(2,1);source.data.set([255,0,0,255,0,0,255,255]);
        createImageBitmap(source,{resizeWidth:4,resizeHeight:1,resizeQuality:'pixelated'}).then(bitmap=>{
            const target=new OffscreenCanvas(4,1).getContext('2d');target.drawImage(bitmap,0,0);
            document.querySelector('output').textContent=[...target.getImageData(0,0,4,1).data].join(',');
        });
    "#,
        "255,0,0,255,255,0,0,255,0,0,255,255,0,0,255,255",
    );
}

#[test]
fn non_integer_pixelated_scaling_uses_second_stage_bilinear_filter() {
    check(
        r#"
        const source=new ImageData(2,1);source.data.set([255,0,0,255,0,0,255,255]);
        createImageBitmap(source,{resizeWidth:3,resizeHeight:1,resizeQuality:'pixelated'}).then(bitmap=>{
            const target=new OffscreenCanvas(3,1).getContext('2d');target.drawImage(bitmap,0,0);
            document.querySelector('output').textContent=[...target.getImageData(0,0,3,1).data].join(',');
        });
    "#,
        "255,0,0,255,128,0,128,255,0,0,255,255",
    );
}

#[test]
fn smooth_resize_filters_in_premultiplied_space_to_avoid_transparent_color_halos() {
    check(
        r#"
        const source=new ImageData(2,1);source.data.set([255,0,0,255,0,0,255,0]);
        createImageBitmap(source,{resizeWidth:1,resizeHeight:1,resizeQuality:'low'}).then(bitmap=>{
            const target=new OffscreenCanvas(1,1).getContext('2d');target.drawImage(bitmap,0,0);
            document.querySelector('output').textContent=[...target.getImageData(0,0,1,1).data].join(',');
        });
    "#,
        "255,0,0,128",
    );
}

#[test]
fn resizing_to_the_same_dimensions_does_not_quantize_straight_alpha() {
    check(
        r#"
        const source=new ImageData(1,1);source.data.set([101,51,25,128]);
        Promise.all(['pixelated','low','medium','high'].map(resizeQuality=>
            createImageBitmap(source,{resizeWidth:1,resizeHeight:1,resizeQuality,premultiplyAlpha:'none'})))
            .then(bitmaps=>{
                const values=bitmaps.map(bitmap=>{
                    const target=new OffscreenCanvas(1,1).getContext('2d');target.drawImage(bitmap,0,0);
                    return [...target.getImageData(0,0,1,1).data].join(',');
                });
                document.querySelector('output').textContent=values.every(value=>value==='101,51,25,128');
            });
    "#,
        "true",
    );
}

#[test]
fn author_close_override_cannot_prevent_internal_transfer_detachment() {
    check(
        r#"
        createImageBitmap(new ImageData(1,1)).then(bitmap=>{
            bitmap.close=()=>{throw Error('author close invoked');};
            const moved=structuredClone(bitmap,{transfer:[bitmap]});
            const copy=structuredClone(moved);
            moved.close=()=>{throw Error('author close invoked');};
            const renderer=new OffscreenCanvas(1,1).getContext('bitmaprenderer');
            renderer.transferFromImageBitmap(moved);
            document.querySelector('output').textContent=[bitmap.width,moved.width,copy.width].join(',');
        });
    "#,
        "0,0,1",
    );
}

#[test]
fn closing_a_bitmap_twice_is_idempotent_but_reading_it_as_source_fails() {
    check(
        r#"
        createImageBitmap(new ImageData(1,1)).then(bitmap=>{
            bitmap.close();bitmap.close();
            const target=new OffscreenCanvas(1,1).getContext('2d');
            let draw='';try{target.drawImage(bitmap,0,0);}catch(error){draw=error.name;}
            return createImageBitmap(bitmap).then(()=>{},error=>{
                document.querySelector('output').textContent=[bitmap.width,bitmap.height,draw,error.name].join(',');
            });
        });
    "#,
        "0,0,InvalidStateError,InvalidStateError",
    );
}
