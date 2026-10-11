//! File API URLs must supply real decoded pixels without an HTTP request.
use super::*;

fn check(code: &str, expected: &str) {
    let (dom, outcome) = execute_html(&format!(
        "<!doctype html><output></output><script>(async()=>{{{code}}})().catch(error=>document.querySelector('output').textContent='failed '+error.name+':'+error.message)</script>"
    ));
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert!(
        outcome.fetch_actions.is_empty(),
        "{:?}",
        outcome.fetch_actions
    );
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        expected
    );
}

const SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="10"><rect width="10" height="10" fill="#204060"/></svg>"##;

#[test]
fn detached_blob_image_supplies_canvas_pixels_and_keeps_decoded_ownership() {
    check(
        &format!(
            r#"
        const url=URL.createObjectURL(new Blob([`{SVG}`],{{type:'image/svg+xml'}}));
        const image=new Image(); image.src=url+'#view'; await image.decode();
        URL.revokeObjectURL(url);
        const canvas=document.createElement('canvas'); canvas.width=20;canvas.height=10;
        const ctx=canvas.getContext('2d');ctx.drawImage(image,0,0);
        const bitmap=await createImageBitmap(image);
        document.querySelector('output').textContent=[image.complete,image.naturalWidth,image.naturalHeight,bitmap.width,
          [...ctx.getImageData(2,2,1,1).data], [...ctx.getImageData(15,2,1,1).data]].join('|');
    "#
        ),
        "true|20|10|20|32,64,96,255|0,0,0,0",
    );
}

#[test]
fn revoked_and_foreign_blob_urls_fail_without_network_fallback() {
    check(
        &format!(
            r#"
        const revoked=URL.createObjectURL(new Blob([`{SVG}`],{{type:'image/svg+xml'}}));
        URL.revokeObjectURL(revoked);
        const results=[];
        for(const url of [revoked,'blob:https://foreign.example/1']) {{
          const image=new Image();image.src=url;
          try {{await image.decode();results.push('incorrect');}}catch(error){{results.push(error.name,image.complete,image.naturalWidth);}}
        }}
        document.querySelector('output').textContent=results.join('|');
    "#
        ),
        "EncodingError|true|0|EncodingError|true|0",
    );
}

#[test]
fn superseded_blob_image_does_not_publish_old_pixels_or_success() {
    check(
        &format!(
            r#"
        const first=URL.createObjectURL(new Blob([`{SVG}`],{{type:'image/svg+xml'}}));
        const second=URL.createObjectURL(new Blob(['broken'],{{type:'image/svg+xml'}}));
        const image=new Image();image.src=first;
        const old=image.decode().then(()=> 'incorrect',error=>error.name);
        image.src=second;
        const next=image.decode().then(()=> 'incorrect',error=>error.name);
        document.querySelector('output').textContent=[await old,await next,image.naturalWidth].join('|');
    "#
        ),
        "EncodingError|EncodingError|0",
    );
}

#[test]
fn blob_image_decoder_uses_private_bytes_not_author_blob_methods() {
    check(
        &format!(
            r#"
        const blob=new Blob([`{SVG}`],{{type:'image/svg+xml'}});
        for(const key of ['arrayBuffer','bytes','size','type','__bytes'])
          Object.defineProperty(blob,key,{{get(){{throw Error('author Blob accessor');}}}});
        const image=new Image();image.src=URL.createObjectURL(blob);await image.decode();
        document.querySelector('output').textContent=image.naturalWidth+'|'+image.naturalHeight;
    "#
        ),
        "20|10",
    );
}

#[test]
fn serialized_svg_with_embedded_font_decodes_through_blob_image_into_canvas() {
    use base64::Engine as _;
    let font = base64::engine::general_purpose::STANDARD
        .encode(include_bytes!("../../../../tests/canvas/fonts/ahem.ttf"));
    check(
        &format!(
            r#"
        const svg=document.createElementNS('http://www.w3.org/2000/svg','svg');
        svg.setAttribute('width','100');svg.setAttribute('height','60');
        svg.innerHTML='<style>@font-face{{font-family:BlobOutline;src:url(data:font/ttf;base64,{font})}}</style><text x="10" y="40" font-family="BlobOutline" font-size="32" fill="red">AA</text>';
        const image=new Image();image.src=URL.createObjectURL(new Blob([new XMLSerializer().serializeToString(svg)],{{type:'image/svg+xml'}}));
        await image.decode();
        const canvas=document.createElement('canvas');canvas.width=100;canvas.height=60;
        const ctx=canvas.getContext('2d');ctx.drawImage(image,0,0);
        document.querySelector('output').textContent=[...ctx.getImageData(12,20,1,1).data].join(',');
    "#
        ),
        "255,0,0,255",
    );
}

#[test]
fn local_blob_images_obey_image_policy_independently_of_connect_policy() {
    for (policy, expected) in [
        ("img-src blob:; connect-src 'none'", "20"),
        ("img-src 'none'; connect-src blob:", "EncodingError"),
    ] {
        check(
            &format!(
                r#"
            const meta=document.createElement('meta');
            meta.setAttribute('http-equiv','Content-Security-Policy');meta.setAttribute('content',"{policy}");
            document.head.appendChild(meta);
            const image=new Image();
            image.src=URL.createObjectURL(new Blob([`{SVG}`],{{type:'image/svg+xml'}}));
            try{{await image.decode();document.querySelector('output').textContent=String(image.naturalWidth);}}
            catch(error){{document.querySelector('output').textContent=error.name;}}
        "#
            ),
            expected,
        );
    }
}
