use super::*;
use windows::{
    Win32::{
        Foundation::HANDLE,
        Graphics::{Direct3D::D3D_DRIVER_TYPE_UNKNOWN, Direct3D11::*, Dxgi::*},
    },
    core::Interface,
};

#[test]
#[ignore = "hardware GPU sharing across the real AppContainer boundary"]
fn contained_gpu_canvas_texture_handoff_preserves_pixels() {
    let mut session = RendererSession::launch(options()).unwrap();
    const HTML: &str = r#"<!doctype html><canvas width=8 height=4></canvas><script>
      const gl=document.querySelector('canvas').getContext('webgl2',{alpha:false,antialias:false,preserveDrawingBuffer:true});
      if(!gl) throw Error('real GLES3 required');
      function draw(){gl.disable(gl.SCISSOR_TEST);gl.clearColor(0,1,0,1);gl.clear(gl.COLOR_BUFFER_BIT);
        gl.enable(gl.SCISSOR_TEST);gl.scissor(0,0,4,2);gl.clearColor(1,0,0,1);gl.clear(gl.COLOR_BUFFER_BIT);
        gl.disable(gl.SCISSOR_TEST); requestAnimationFrame(draw);}
      draw();
    </script>"#;
    let initial = load_scripted_html_document(&session, 77, HTML);
    session
        .set_gpu_canvas_policy(initial.document, true)
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut timer = initial.next_timer_micros;
    let mut advance = true;
    let frame = loop {
        assert!(
            Instant::now() < deadline,
            "contained GPU export did not arrive"
        );
        if advance {
            session
                .advance_time(
                    initial.document,
                    Duration::from_micros(timer.unwrap_or(16_667)),
                    1,
                )
                .unwrap();
            advance = false;
        }
        match session
            .wait_for_event(Duration::from_secs(3))
            .unwrap_or_else(|error| panic!("{error}: {:?}", session.snapshot()))
        {
            RendererEvent::Presentation(value) => {
                assert!(
                    value.runtime.errors.is_empty(),
                    "{:?}",
                    value.runtime.errors
                );
                acknowledge(&session, &value);
                timer = value.next_timer_micros;
                if let Some(canvas) = value.gpu_canvases.first() {
                    break canvas.frame;
                }
                advance = true;
            }
            RendererEvent::RuntimeUpdate(value) => {
                timer = value.next_timer_micros;
                advance = true;
            }
            RendererEvent::Diagnostic { .. } => {}
            other => panic!("unexpected GPU fixture event: {other:?}"),
        }
    };
    let factory: IDXGIFactory1 = unsafe { CreateDXGIFactory1() }.unwrap();
    let adapter = (0..16)
        .find_map(|index| {
            let adapter = unsafe { factory.EnumAdapters1(index) }.ok()?;
            let desc = unsafe { adapter.GetDesc1() }.ok()?;
            (desc.AdapterLuid.LowPart == frame.adapter_low
                && desc.AdapterLuid.HighPart == frame.adapter_high)
                .then_some(adapter)
        })
        .expect("exported adapter is enumerated");
    let mut device = None;
    let mut context = None;
    unsafe {
        D3D11CreateDevice(
            &adapter,
            D3D_DRIVER_TYPE_UNKNOWN,
            Default::default(),
            D3D11_CREATE_DEVICE_BGRA_SUPPORT,
            None,
            D3D11_SDK_VERSION,
            Some(&mut device),
            None,
            Some(&mut context),
        )
        .unwrap();
    }
    let device = device.unwrap();
    let context = context.unwrap();
    let mut shared: Option<ID3D11Texture2D> = None;
    unsafe {
        device
            .OpenSharedResource(HANDLE(frame.handle as usize as *mut _), &mut shared)
            .unwrap();
    }
    let shared = shared.unwrap();
    let mutex: IDXGIKeyedMutex = shared.cast().unwrap();
    // This test deliberately waits for the fixed fixture; ordinary composition polls with zero timeout.
    let acquired = unsafe { (mutex.vtable().AcquireSync)(mutex.as_raw(), frame.key, 1000) };
    assert_eq!(acquired.0, 0);
    let mut desc = D3D11_TEXTURE2D_DESC::default();
    unsafe {
        shared.GetDesc(&mut desc);
    }
    assert_eq!((desc.Width, desc.Height), (8, 4));
    desc.Usage = D3D11_USAGE_STAGING;
    desc.BindFlags = 0;
    desc.MiscFlags = 0;
    desc.CPUAccessFlags = D3D11_CPU_ACCESS_READ.0 as u32;
    let mut staging = None;
    unsafe {
        device
            .CreateTexture2D(&desc, None, Some(&mut staging))
            .unwrap();
    }
    let staging = staging.unwrap();
    unsafe {
        context.CopyResource(&staging, &shared);
        context.Flush();
        mutex.ReleaseSync(0).unwrap();
    }
    let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
    unsafe {
        context
            .Map(&staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))
            .unwrap();
    }
    let mut pixels = Vec::new();
    for row in 0..4 {
        pixels.extend_from_slice(unsafe {
            std::slice::from_raw_parts(
                (mapped.pData as *const u8).add(row * mapped.RowPitch as usize),
                32,
            )
        });
    }
    unsafe {
        context.Unmap(&staging, 0);
    }
    for y in 0..4 {
        for x in 0..8 {
            let expected = if y >= 2 && x < 4 {
                [0, 0, 255, 255]
            } else {
                [0, 255, 0, 255]
            };
            assert_eq!(&pixels[(y * 8 + x) * 4..(y * 8 + x + 1) * 4], &expected);
        }
    }
    session
        .set_gpu_canvas_policy(initial.document, false)
        .unwrap();
    // No clock advance or author callback is needed to repair a static frame.
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        assert!(Instant::now() < deadline, "CPU repair did not arrive");
        match session.wait_for_event(Duration::from_secs(3)).unwrap() {
            RendererEvent::Presentation(value) => {
                acknowledge(&session, &value);
                assert!(value.runtime.errors.is_empty());
                if value.gpu_canvases.is_empty()
                    && let Some(image) = value
                        .images
                        .iter()
                        .find(|image| image.url.starts_with("breeze-internal:canvas:"))
                {
                    assert_eq!(image.image.bgra.as_ref(), pixels.as_slice());
                    break;
                }
            }
            RendererEvent::RuntimeUpdate(_) | RendererEvent::Diagnostic { .. } => {}
            other => panic!("unexpected CPU repair event: {other:?}"),
        }
    }
    session.shutdown().unwrap();
}
