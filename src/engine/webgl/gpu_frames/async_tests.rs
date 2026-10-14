use super::super::{ApiVersion, Options, backend_policy::Backend};
use super::*;
use std::time::{Duration, Instant};

#[test]
fn pending_gpu_frame_is_completed_without_another_author_draw() {
    super::super::session::run_native_test(|| {
        let mut context = WebGl::new_with_backend(
            4,
            2,
            Options {
                api: ApiVersion::Two,
                antialias: true,
                alpha: false,
                preserve: false,
                ..Default::default()
            },
            Backend::Software,
        )
        .unwrap();
        unsafe {
            gl::ClearColor(0.0, 1.0, 0.0, 1.0);
            gl::Clear(gl::COLOR_BUFFER_BIT);
        }
        let first = context.poll_gpu_frame().unwrap();
        assert!(first.supported && first.submitted && first.pending);
        assert!(
            first.frame.is_none(),
            "no key may be exposed before fence completion"
        );
        context.presented();
        // The GPU-side source dependency must make this immediate implicit
        // clear follow the private copy, including the MSAA resolve.
        assert_eq!(
            context.surface.snapshot().unwrap(),
            [0, 0, 0, 255].repeat(8)
        );
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            let export = context.poll_gpu_frame().unwrap();
            assert!(
                !export.submitted,
                "polling must not copy the implicit clear"
            );
            if !export.pending {
                assert!(export.frame.is_some());
                break;
            }
            assert!(Instant::now() < deadline);
            std::thread::yield_now();
        }
        let bitmap = context.last_gpu_bitmap().unwrap().unwrap();
        assert_eq!(bitmap.2, [0, 255, 0, 255].repeat(8));
        assert_eq!(unsafe { gl::GetError() }, gl::NO_ERROR);
    });
}

#[test]
fn synchronous_repair_and_retirement_complete_pending_copies() {
    super::super::session::run_native_test(|| {
        let initial = RESIDENT_BYTES.load(Ordering::Relaxed);
        let mut context = WebGl::new_with_backend(
            4,
            2,
            Options {
                api: ApiVersion::Two,
                antialias: false,
                preserve: false,
                ..Default::default()
            },
            Backend::Software,
        )
        .unwrap();
        unsafe {
            gl::ClearColor(1.0, 0.0, 0.0, 1.0);
            gl::Clear(gl::COLOR_BUFFER_BIT);
        }
        assert!(context.poll_gpu_frame().unwrap().pending);
        context.presented();
        let bitmap = context.last_gpu_bitmap().unwrap().unwrap();
        assert_eq!(bitmap.2, [255, 0, 0, 255].repeat(8));
        assert_eq!(RESIDENT_BYTES.load(Ordering::Relaxed), initial);
        context.surface.presentation_dirty = true;
        assert!(context.poll_gpu_frame().unwrap().submitted);
        assert_eq!(context.gpu_frames.as_ref().unwrap().slots.len(), SLOTS);
        drop(context);
        assert_eq!(RESIDENT_BYTES.load(Ordering::Relaxed), initial);
    });
}
