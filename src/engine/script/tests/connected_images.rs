//! Renderer-owned image bytes, completion, retargeting, and Canvas consumers.
use super::*;
use crate::engine::{DecodedImage, Page};
use std::sync::Arc;

fn image() -> DecodedImage {
    DecodedImage {
        width: 2,
        height: 1,
        bgra: Arc::from([90, 60, 30, 255, 16, 32, 64, 128]),
    }
}

fn runtime(html: &str, clean: bool) -> (Page, ScriptRuntime) {
    let mut page = Page::parse_scripted(html, "https://example.com/");
    page.images
        .insert("https://example.com/a.png".into(), image());
    page.set_image_origin_clean("https://example.com/a.png", clean);
    let mut runtime = ScriptRuntime::new(page.dom.document.clone(), &page.source_url);
    page.synchronize_script_images(&mut runtime);
    let initial = runtime.execute_initial_with_loader(&[], None);
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    (page, runtime)
}

fn run(page: &Page, runtime: &mut ScriptRuntime, source: &str) -> String {
    let outcome = runtime.execute_additional_with_loader(
        &[ScriptInput {
            source_url: "https://example.com/#probe".into(),
            code: source.into(),
            node: page.dom.elements_named("script").next().unwrap(),
            kind: ScriptKind::Classic,
            fetch_options: ScriptFetchOptions::for_kind(ScriptKind::Classic),
            finish_lifecycle: true,
        }],
        None,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    page.dom
        .elements_named("output")
        .next()
        .unwrap()
        .text_content()
}

#[test]
fn decoded_connected_images_draw_patterns_and_create_independent_bitmaps() {
    let (page, mut runtime) = runtime(
        "<img id=picture src=/a.png width=999><output></output><script></script>",
        true,
    );
    assert_eq!(
        run(
            &page,
            &mut runtime,
            r#"
        const picture=document.querySelector('img'), result=document.querySelector('output');
        const canvas=document.createElement('canvas');canvas.width=2;canvas.height=1;
        const ctx=canvas.getContext('2d');ctx.drawImage(picture,0,0);
        const first=[...ctx.getImageData(0,0,2,1).data].join();
        ctx.clearRect(0,0,2,1);ctx.fillStyle=ctx.createPattern(picture,'repeat');ctx.fillRect(0,0,2,1);
        const pattern=[...ctx.getImageData(0,0,2,1).data].join();
        createImageBitmap(picture).then(bitmap=>{
            picture.src='/not-loaded.png';ctx.clearRect(0,0,2,1);ctx.drawImage(bitmap,0,0);
            result.textContent=[first,pattern,[...ctx.getImageData(0,0,2,1).data].join(),bitmap.width].join('|');
        });
    "#
        ),
        "30,60,90,255,128,64,32,128|30,60,90,255,128,64,32,128|30,60,90,255,128,64,32,128|2"
    );
}

#[test]
fn origin_policy_survives_mutable_properties_and_rejects_before_drawing() {
    let (page, mut runtime) = runtime("<img src=/a.png><output></output><script></script>", false);
    assert_eq!(
        run(
            &page,
            &mut runtime,
            r#"
        const image=document.querySelector('img'), output=document.querySelector('output');
        image.crossOrigin='anonymous';
        Object.defineProperties(image,{naturalWidth:{value:100},naturalHeight:{value:100},complete:{value:true}});
        const canvas=document.createElement('canvas');canvas.width=2;canvas.height=1;
        const ctx=canvas.getContext('2d');ctx.fillStyle='red';ctx.fillRect(0,0,2,1);
        const errors=[];for(const fn of [()=>ctx.drawImage(image,0,0),()=>ctx.createPattern(image,'repeat')])
            try{fn();errors.push('missing');}catch(error){errors.push(error.name);}
        createImageBitmap(image).then(()=>output.textContent='incorrect',error=>{
            output.textContent=[...errors,error.name,[...ctx.getImageData(0,0,1,1).data].join()].join('|');
        });
    "#
        ),
        "SecurityError|SecurityError|SecurityError|255,0,0,255"
    );
}

#[test]
fn source_replacement_and_missing_policy_never_reuse_previous_pixels() {
    let (page, mut runtime) = runtime("<img src=/a.png><output></output><script></script>", true);
    assert_eq!(
        run(
            &page,
            &mut runtime,
            r#"
        const image=document.querySelector('img'), output=document.querySelector('output');
        const canvas=document.createElement('canvas');canvas.width=2;canvas.height=1;
        const ctx=canvas.getContext('2d');ctx.fillStyle='red';ctx.fillRect(0,0,2,1);
        image.src='/pending.png';ctx.globalCompositeOperation='copy';ctx.drawImage(image,0,0);
        output.textContent=[image.complete,image.naturalWidth,[...ctx.getImageData(0,0,1,1).data].join()].join('|');
    "#
        ),
        "false|0|255,0,0,255"
    );
    runtime.set_document_images(&page.images, &HashMap::new());
    assert_eq!(
        run(
            &page,
            &mut runtime,
            "image.src='/a.png';output.textContent=[image.complete,image.naturalWidth].join('|');"
        ),
        "false|0"
    );
}

#[test]
fn private_native_handles_and_attributes_ignore_author_spoofing() {
    let (page, mut runtime) = runtime("<img src=/a.png><output></output><script></script>", true);
    assert_eq!(
        run(
            &page,
            &mut runtime,
            r#"
        const image=document.querySelector('img'), output=document.querySelector('output');
        image.__nodeid=0;image.__localName='canvas';image.getAttribute=()=>'/pending.png';
        Object.defineProperties(image,{src:{value:'https://foreign.test/'},currentSrc:{value:'bad'},
            width:{value:99999},naturalWidth:{value:99999}});
        const canvas=document.createElement('canvas');canvas.width=2;canvas.height=1;
        const ctx=canvas.getContext('2d');ctx.drawImage(image,0,0);
        output.textContent=[...ctx.getImageData(0,0,1,1).data].join();
    "#
        ),
        "30,60,90,255"
    );
}

#[test]
fn responsive_source_selection_is_shared_with_the_page_loader() {
    let (page, mut runtime) = runtime(
        "<picture><source srcset='/a.png 1x'><img src=/wrong.png></picture><output></output><script></script>",
        true,
    );
    assert_eq!(
        run(
            &page,
            &mut runtime,
            r#"
        const image=document.querySelector('img'), output=document.querySelector('output');
        const canvas=document.createElement('canvas');canvas.width=2;canvas.height=1;
        const ctx=canvas.getContext('2d');ctx.drawImage(image,0,0);
        output.textContent=[image.currentSrc,image.naturalWidth,[...ctx.getImageData(0,0,1,1).data].join()].join('|');
    "#
        ),
        "https://example.com/a.png|2|30,60,90,255"
    );
}

#[test]
fn data_images_are_published_before_initial_script_execution() {
    let mut encoded = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
        2,
        1,
        image::Rgba([30, 60, 90, 255]),
    ))
    .write_to(&mut encoded, image::ImageFormat::Png)
    .unwrap();
    use base64::Engine;
    let base64 = base64::engine::general_purpose::STANDARD.encode(encoded.get_ref());
    let mut page = Page::parse_scripted(
        &format!(
            r#"<img src="data:image/png;base64,{base64}"><output></output><script>
        const canvas=document.createElement('canvas');canvas.width=2;canvas.height=1;
        const ctx=canvas.getContext('2d');ctx.drawImage(document.querySelector('img'),0,0);
        document.querySelector('output').textContent=[...ctx.getImageData(0,0,1,1).data].join();
    </script>"#
        ),
        "https://example.com/",
    );
    let outcome = page.execute_scripts();
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        page.dom
            .elements_named("output")
            .next()
            .unwrap()
            .text_content(),
        "30,60,90,255"
    );
}
