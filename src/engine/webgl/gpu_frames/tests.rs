use super::super::{ApiVersion, Options, backend_policy::Backend};
use super::*;
use windows::Win32::Graphics::Direct3D11::*;

#[test]
fn gpu_frames_preserve_alpha_rows_and_author_state() {
    super::super::session::run_native_test(|| {
        for (alpha, premultiplied, antialias) in [
            (false, true, false),
            (true, true, false),
            (true, false, false),
            (false, true, true),
            (true, true, true),
            (true, false, true),
        ] {
            let mut context = WebGl::new_with_backend(
                4,
                2,
                Options {
                    api: ApiVersion::Two,
                    alpha,
                    premultiplied_alpha: premultiplied,
                    preserve: true,
                    antialias,
                    depth: false,
                    ..Default::default()
                },
                Backend::Software,
            )
            .unwrap();
            let colors = [
                [255, 80, 16, 64],
                [32, 64, 16, 128],
                [1, 254, 127, 1],
                [200, 80, 90, 0],
            ];
            unsafe {
                gl::Enable(gl::SCISSOR_TEST);
                for y in 0..2 {
                    for x in 0..4 {
                        let color = colors[(x + y) % 4];
                        gl::Scissor(x as i32, y as i32, 1, 1);
                        gl::ClearColor(
                            color[0] as f32 / 255.0,
                            color[1] as f32 / 255.0,
                            color[2] as f32 / 255.0,
                            color[3] as f32 / 255.0,
                        );
                        gl::Clear(gl::COLOR_BUFFER_BIT);
                    }
                }
                gl::Viewport(1, 2, 3, 4);
                gl::ActiveTexture(gl::TEXTURE0 + 3);
                gl::Enable(0x8c89);
                gl::ColorMask(0, 1, 0, 1);
            }
            let core = context.core.as_ref().unwrap();
            let mut query = 0;
            unsafe {
                (core.gen_queries)(1, &mut query);
                (core.begin_query)(0x8d6a, query);
            }
            let frame = context.gpu_frame().unwrap().unwrap();
            let core = context.core.as_ref().unwrap();
            unsafe {
                (core.end_query)(0x8d6a);
                gl::Finish();
            }
            let mut samples = 1;
            unsafe {
                (core.query_result)(query, 0x8866, &mut samples);
            }
            assert_eq!(
                samples, 0,
                "private draws must not enter the author's occlusion query"
            );
            let mut viewport = [0; 4];
            let mut active = 0;
            let mut mask = [0; 4];
            unsafe {
                gl::GetIntegerv(gl::VIEWPORT, viewport.as_mut_ptr());
                gl::GetIntegerv(gl::ACTIVE_TEXTURE, &mut active);
                gl::GetBooleanv(gl::COLOR_WRITEMASK, mask.as_mut_ptr());
            }
            assert_eq!(viewport, [1, 2, 3, 4]);
            assert_eq!(active, gl::TEXTURE0 as i32 + 3);
            assert_eq!(mask, [0, 1, 0, 1]);
            assert_ne!(unsafe { gl::IsEnabled(0x8c89) }, 0);
            let device = context.native.d3d_device().unwrap();
            let commands = unsafe { device.GetImmediateContext() }.unwrap();
            let mut shared: Option<ID3D11Texture2D> = None;
            unsafe {
                device
                    .OpenSharedResource(
                        windows::Win32::Foundation::HANDLE(frame.handle as usize as *mut _),
                        &mut shared,
                    )
                    .unwrap();
            }
            let shared = shared.unwrap();
            let mutex: windows::Win32::Graphics::Dxgi::IDXGIKeyedMutex = shared.cast().unwrap();
            assert!(try_acquire(&mutex, frame.key).unwrap());
            let mut desc = D3D11_TEXTURE2D_DESC::default();
            unsafe {
                shared.GetDesc(&mut desc);
            }
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
                commands.CopyResource(&staging, &shared);
                commands.Flush();
                mutex.ReleaseSync(0).unwrap();
            }
            let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
            unsafe {
                commands
                    .Map(&staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))
                    .unwrap();
            }
            let mut pixels = Vec::new();
            for row in 0..2 {
                pixels.extend_from_slice(unsafe {
                    std::slice::from_raw_parts(
                        (mapped.pData as *const u8).add(row * mapped.RowPitch as usize),
                        16,
                    )
                });
            }
            unsafe {
                commands.Unmap(&staging, 0);
            }
            for y in 0..2 {
                for x in 0..4 {
                    let color = colors[(x + (1 - y)) % 4];
                    let a = if alpha { color[3] } else { 255 };
                    let convert = |c: u8| {
                        if !alpha {
                            c
                        } else if premultiplied {
                            c.min(a)
                        } else {
                            ((u16::from(c) * u16::from(a) + 127) / 255) as u8
                        }
                    };
                    let expected = [convert(color[2]), convert(color[1]), convert(color[0]), a];
                    assert_eq!(
                        &pixels[(y * 4 + x) * 4..(y * 4 + x + 1) * 4],
                        &expected,
                        "alpha={alpha}, premultiplied={premultiplied}"
                    );
                }
            }
        }
    });
}

#[test]
fn gpu_frames_do_not_wait_for_a_consumer_or_reuse_an_old_generation() {
    super::super::session::run_native_test(|| {
        let mut context = WebGl::new_with_backend(
            4,
            2,
            Options {
                api: ApiVersion::Two,
                antialias: false,
                ..Default::default()
            },
            Backend::Software,
        )
        .unwrap();
        let baseline = RESIDENT_BYTES.load(Ordering::Relaxed);
        let mut held = Vec::new();
        let mut first = None;
        let device = context.native.d3d_device().unwrap();
        for _ in 0..SLOTS {
            let frame = context.gpu_frame().unwrap().unwrap();
            let mut texture: Option<ID3D11Texture2D> = None;
            unsafe {
                device
                    .OpenSharedResource(
                        windows::Win32::Foundation::HANDLE(frame.handle as usize as *mut _),
                        &mut texture,
                    )
                    .unwrap();
            }
            let mutex: windows::Win32::Graphics::Dxgi::IDXGIKeyedMutex =
                texture.unwrap().cast().unwrap();
            assert!(try_acquire(&mutex, frame.key).unwrap());
            if first.is_none() {
                first = Some(frame);
            }
            held.push(mutex);
        }
        assert!(
            context.gpu_frame().unwrap().is_none(),
            "all slots are consumer-owned"
        );
        unsafe {
            held[0].ReleaseSync(0).unwrap();
        }
        let old = first.unwrap();
        let next = context.gpu_frame().unwrap().unwrap();
        assert_eq!(next.surface, old.surface);
        assert!(next.key > old.key);
        assert!(
            !try_acquire(&held[0], old.key).unwrap(),
            "stale metadata cannot own the rewritten slot"
        );
        assert!(try_acquire(&held[0], next.key).unwrap());
        for mutex in held {
            unsafe {
                mutex.ReleaseSync(0).unwrap();
            }
        }
        drop(context);
        assert_eq!(RESIDENT_BYTES.load(Ordering::Relaxed), baseline);
    });
}
