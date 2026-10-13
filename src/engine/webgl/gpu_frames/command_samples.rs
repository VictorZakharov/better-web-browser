//! Opt-in completed commands with rotated order and untimed pixel verification.
use super::super::{ApiVersion, Options, backend_policy::Backend};
use super::*;
use crate::gpu_composition::{Device, Fence, Frames, Target};
use crate::renderer_protocol::PresentedGpuCanvas;
use std::time::{Duration, Instant};
use windows::Win32::Graphics::Direct2D::{Common::*, *};

#[test]
#[ignore = "release hardware command measurements; one GPU worker, run alone with --nocapture"]
fn gpu_canvas_completed_command_samples() {
    super::super::session::run_native_test(|| {
        for (width, height) in [(128, 128), (1382, 760), (2048, 1024)] {
            let mut context = WebGl::new_with_backend(
                width,
                height,
                Options {
                    api: ApiVersion::Two,
                    alpha: true,
                    premultiplied_alpha: true,
                    antialias: false,
                    preserve: true,
                    depth: false,
                    ..Default::default()
                },
                Backend::Hardware(None),
            )
            .unwrap();
            prepare(&context, width, height);
            let first = context.gpu_frame().unwrap().unwrap();
            let device = Device::hardware(Some((first.adapter_low, first.adapter_high))).unwrap();
            let target = Target::new(&device, width, height).unwrap();
            let mut frames = Frames::default();
            // Clear-color quantization can differ by one byte between hardware
            // and WARP. Verify against the actual authored bytes, not a guessed
            // floating-point tie result.
            let mut rgba = context.surface.snapshot().unwrap();
            crate::engine::image_decode::unpremultiply_rgba(&mut rgba);
            let expected = crate::engine::image_decode::RasterImage {
                width,
                height,
                rgba,
                rgba16: None,
            }
            .into_premultiplied_bgra()
            .bgra
            .to_vec();
            complete(&mut context, &device, &target, &mut frames, false);
            assert_eq!(target.read_bgra(&device).unwrap(), expected);
            assert_eq!(&expected[..4], &[0, 255, 0, 255]);
            for round in 0..3 {
                for gpu in if round % 2 == 0 {
                    [false, true]
                } else {
                    [true, false]
                } {
                    for sample in 0..15 {
                        // Finish authored work before timing presentation responsibilities.
                        prepare(&context, width, height);
                        let started = Instant::now();
                        complete(&mut context, &device, &target, &mut frames, gpu);
                        let elapsed = started.elapsed().as_secs_f64() * 1000.0;
                        assert_eq!(
                            target.read_bgra(&device).unwrap(),
                            expected,
                            "completed pixels differ"
                        );
                        if sample >= 3 {
                            println!(
                                "gpu_canvas_command width={width} height={height} round={round} gpu={gpu} sample={} completed_ms={elapsed:.6}",
                                sample - 3
                            );
                        }
                    }
                }
            }
        }
    });
}

fn prepare(context: &WebGl, width: u32, height: u32) {
    context.native.make_current().unwrap();
    unsafe {
        gl::BindFramebuffer(gl::FRAMEBUFFER, context.surface.framebuffer);
        gl::Disable(gl::SCISSOR_TEST);
        gl::ClearColor(0.25, 0.125, 0.0625, 0.5);
        gl::Clear(gl::COLOR_BUFFER_BIT);
        gl::Enable(gl::SCISSOR_TEST);
        gl::Scissor(0, height as i32 / 2, width as i32, height as i32 / 2);
        gl::ClearColor(0.0, 1.0, 0.0, 1.0);
        gl::Clear(gl::COLOR_BUFFER_BIT);
        gl::Disable(gl::SCISSOR_TEST);
        gl::Finish();
        assert_eq!(gl::GetError(), gl::NO_ERROR);
    }
}

fn complete(context: &mut WebGl, device: &Device, target: &Target, frames: &mut Frames, gpu: bool) {
    let bitmap = if gpu {
        let frame = context.gpu_frame().unwrap().unwrap();
        let canvas = PresentedGpuCanvas {
            url: format!("breeze-internal:canvas:1:{}x{}", frame.width, frame.height),
            frame,
        };
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            frames
                .update(device, std::slice::from_ref(&canvas))
                .unwrap();
            if frames.ready(std::slice::from_ref(&canvas)) {
                break;
            }
            assert!(Instant::now() < deadline, "GPU frame failed to complete");
            std::thread::yield_now();
        }
        frames.bitmap(&canvas.url).unwrap().clone()
    } else {
        context.native.make_current().unwrap();
        let mut rgba = context.surface.snapshot().unwrap();
        crate::engine::image_decode::unpremultiply_rgba(&mut rgba);
        let image = crate::engine::image_decode::RasterImage {
            width: target.width,
            height: target.height,
            rgba,
            rgba16: None,
        }
        .into_premultiplied_bgra();
        device
            .upload(image.width, image.height, &image.bgra)
            .unwrap()
    };
    unsafe {
        device.d2d.SetTarget(&target.bitmap);
        device.d2d.BeginDraw();
        device.d2d.Clear(Some(&D2D1_COLOR_F::default()));
        device.d2d.DrawBitmap(
            &bitmap,
            None,
            1.0,
            D2D1_INTERPOLATION_MODE_NEAREST_NEIGHBOR,
            None,
            None,
        );
        device.d2d.EndDraw(None, None).unwrap();
    }
    let fence = Fence::submit(device).unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    while !fence.complete(device).unwrap() {
        assert!(
            Instant::now() < deadline,
            "GPU composition failed to complete"
        );
        std::thread::yield_now();
    }
}
