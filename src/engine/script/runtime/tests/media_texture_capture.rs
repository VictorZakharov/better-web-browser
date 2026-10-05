//! Granted capture pixels must be visible in loadeddata, and revoked before later script reads.
use super::*;
use crate::engine::{
    DecodedImage,
    script::{ScriptFetchOptions, ScriptKind},
};
use crate::renderer_protocol::{DocumentId, MediaCaptureEvent, MediaCaptureUpdate};

#[test]
#[cfg(windows)]
fn capture_video_texture_observes_granted_frames_disabled_tracks_and_source_replacement() {
    let dom = dom::parse_with_scripting(
        r#"<body><video id="preview"></video><script>
        const preview=document.getElementById('preview');
        const gl=new OffscreenCanvas(2,2).getContext('webgl2');
        gl.bindTexture(gl.TEXTURE_2D,gl.createTexture());
        const fb=gl.createFramebuffer();gl.bindFramebuffer(gl.FRAMEBUFFER,fb);
        function upload() {
            gl.texImage2D(gl.TEXTURE_2D,0,gl.RGBA8,gl.RGBA,gl.UNSIGNED_BYTE,preview);
            const error=gl.getError();if(error)return 'error:'+error;
            const t=gl.getParameter(gl.TEXTURE_BINDING_2D);
            gl.framebufferTexture2D(gl.FRAMEBUFFER,gl.COLOR_ATTACHMENT0,gl.TEXTURE_2D,t,0);
            const pixels=new Uint8Array(16);gl.readPixels(0,0,2,2,gl.RGBA,gl.UNSIGNED_BYTE,pixels);
            if(gl.getError())throw Error('capture readback');return pixels.join();
        }
        preview.addEventListener('loadeddata',()=>document.body.setAttribute('data-first',upload()));
        navigator.mediaDevices.getUserMedia({video:true}).then(stream=>{
            window.stream=stream;preview.srcObject=stream;
        });
    </script></body>"#,
        true,
    );
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    let initial = runtime.execute_initial(&script_inputs(&dom));
    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    let request_id = initial.media_device_actions[0].request_id;
    let granted = runtime.deliver_media_capture_update(MediaCaptureUpdate {
        document: DocumentId::new(1).unwrap(),
        request_id,
        event: MediaCaptureEvent::Started {
            camera: true,
            microphone: false,
        },
    });
    assert!(granted.errors.is_empty(), "{:?}", granted.errors);
    let image = DecodedImage {
        width: 2,
        height: 2,
        bgra: vec![
            0, 0, 255, 255, 0, 255, 0, 255, 255, 0, 0, 255, 255, 255, 255, 255,
        ]
        .into(),
    };
    runtime.set_capture_media_images(request_id, &image);
    let event = runtime.deliver_media_capture_frame_info(request_id, 2, 2, 10_000_000);
    assert!(event.errors.is_empty(), "{:?}", event.errors);
    let body = dom.elements_named("body").next().unwrap();
    assert_eq!(
        body.attr("data-first").as_deref(),
        Some("255,0,0,255,0,255,0,255,0,0,255,255,255,255,255,255")
    );

    let evaluate = |runtime: &mut ScriptRuntime, code: &str| {
        let outcome = runtime.execute_additional_with_loader(
            &[ScriptInput {
                node: dom.elements_named("script").next().unwrap(),
                source_url: "https://example.com/#video-texture".into(),
                code: code.into(),
                kind: ScriptKind::Classic,
                fetch_options: ScriptFetchOptions::for_kind(ScriptKind::Classic),
                finish_lifecycle: true,
            }],
            None,
        );
        assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    };
    evaluate(
        &mut runtime,
        r#"
        stream.getVideoTracks()[0].enabled=false;
        document.body.setAttribute('data-disabled',upload());
    "#,
    );
    assert_eq!(
        body.attr("data-disabled").as_deref(),
        Some("0,0,0,255,0,0,0,255,0,0,0,255,0,0,0,255")
    );
    runtime.set_capture_media_images(request_id, &image);
    evaluate(
        &mut runtime,
        "document.body.setAttribute('data-disabled-next',upload());",
    );
    assert_eq!(body.attr("data-disabled-next"), body.attr("data-disabled"));
    evaluate(&mut runtime, "stream.getVideoTracks()[0].enabled=true;");
    runtime.set_capture_media_images(request_id, &image);
    evaluate(
        &mut runtime,
        "document.body.setAttribute('data-enabled',upload());",
    );
    assert_eq!(body.attr("data-enabled"), body.attr("data-first"));
    evaluate(
        &mut runtime,
        r#"
        preview.srcObject=null;
        document.body.setAttribute('data-revoked',upload());
    "#,
    );
    assert_eq!(body.attr("data-revoked").as_deref(), Some("error:1281"));
    runtime.set_capture_media_images(request_id, &image);
    evaluate(
        &mut runtime,
        "document.body.setAttribute('data-retired',upload());",
    );
    assert_eq!(body.attr("data-retired"), body.attr("data-revoked"));
}

#[test]
#[cfg(windows)]
fn opaque_video_origin_policy_survives_attribute_changes_and_decoder_budget_failures() {
    let dom = dom::parse_with_scripting(
        r#"<body><video></video><script>
        const video=document.querySelector('video');
        const gl=new OffscreenCanvas(1,1).getContext('webgl2');gl.bindTexture(gl.TEXTURE_2D,gl.createTexture());
        function upload() {
            try{gl.texImage2D(gl.TEXTURE_2D,0,gl.RGBA8,gl.RGBA,gl.UNSIGNED_BYTE,video);
                return 'gl:'+gl.getError();}catch(e){return e.name;}
        }
    </script></body>"#,
        true,
    );
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.com/");
    assert!(
        runtime
            .execute_initial(&script_inputs(&dom))
            .errors
            .is_empty()
    );
    let node = dom.elements_named("video").next().unwrap().id();
    runtime.set_media_image(
        node,
        DecodedImage {
            width: 1,
            height: 1,
            bgra: vec![1, 2, 3, 255].into(),
        },
        false,
    );
    let evaluate = |runtime: &mut ScriptRuntime, code: &str| {
        let outcome = runtime.execute_additional_with_loader(
            &[input(
                &dom.elements_named("script").next().unwrap(),
                "video-policy",
                code,
                true,
            )],
            None,
        );
        assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    };
    evaluate(
        &mut runtime,
        r#"
        video.crossOrigin='anonymous';Object.defineProperty(video,'currentSrc',{value:'https://example.com/safe.mp4'});
        document.body.setAttribute('data-policy',upload());
    "#,
    );
    let body = dom.elements_named("body").next().unwrap();
    assert_eq!(body.attr("data-policy").as_deref(), Some("SecurityError"));
    runtime.set_media_image(
        node,
        DecodedImage {
            width: 1,
            height: 1,
            bgra: vec![1, 2, 3].into(),
        },
        true,
    );
    evaluate(
        &mut runtime,
        "document.body.setAttribute('data-budget',upload());",
    );
    assert_eq!(body.attr("data-budget").as_deref(), Some("gl:1281"));
}
