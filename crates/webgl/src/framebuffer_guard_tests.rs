//! Independent native read/draw bindings survive every private surface operation.
use super::api_version_tests::{call, version_two};
use super::framebuffer_guard::{DRAW, READ, READ_BINDING};
use super::*;

fn binding(pname: u32) -> u32 {
    let mut value = 0;
    unsafe {
        gl::GetIntegerv(pname, &mut value);
    }
    value as u32
}
#[test]
fn webgl2_private_capture_preserves_independent_framebuffers_and_default_read_route() {
    session::run_native_test(|| {
        let mut context = version_two();
        let read_buffer = core_entries::CoreEntries::read_buffer_entry().unwrap();
        unsafe {
            read_buffer(gl::NONE);
        }
        let mut objects = [0; 2];
        unsafe {
            gl::GenFramebuffers(2, objects.as_mut_ptr());
            gl::BindFramebuffer(READ, objects[0]);
            gl::BindFramebuffer(DRAW, objects[1]);
        }
        let pixels = context.surface.snapshot().unwrap();
        assert!(pixels.iter().all(|byte| *byte == 0));
        assert_eq!(binding(READ_BINDING), objects[0]);
        assert_eq!(binding(gl::FRAMEBUFFER_BINDING), objects[1]);
        unsafe {
            gl::BindFramebuffer(READ, context.surface.framebuffer);
        }
        assert_eq!(binding(0x0c02), gl::NONE);
        unsafe {
            gl::BindFramebuffer(READ, objects[0]);
        }
        context.clear_default_surface();
        assert_eq!(binding(READ_BINDING), objects[0]);
        assert_eq!(binding(gl::FRAMEBUFFER_BINDING), objects[1]);
        assert_eq!(unsafe { gl::GetError() }, gl::NO_ERROR);
        unsafe {
            gl::BindFramebuffer(gl::FRAMEBUFFER, context.surface.framebuffer);
            gl::DeleteFramebuffers(2, objects.as_ptr());
        }
    });
}

#[test]
fn webgl2_resize_maps_old_private_read_binding_to_new_surface_and_keeps_author_read_binding() {
    session::run_native_test(|| {
        let mut context = version_two();
        let private = context.surface.framebuffer;
        call(&mut context, "resize", &[8, 8], "");
        assert_ne!(context.surface.framebuffer, private);
        assert_eq!(binding(READ_BINDING), context.surface.framebuffer);
        assert_eq!(
            binding(gl::FRAMEBUFFER_BINDING),
            context.surface.framebuffer
        );
        let mut read = 0;
        unsafe {
            gl::GenFramebuffers(1, &mut read);
            gl::BindFramebuffer(READ, read);
        }
        call(&mut context, "resize", &[4, 4], "");
        assert_eq!(binding(READ_BINDING), read);
        assert_eq!(
            binding(gl::FRAMEBUFFER_BINDING),
            context.surface.framebuffer
        );
        assert_eq!(call(&mut context, "getError", &[], ""), json!(0));
        unsafe {
            gl::BindFramebuffer(READ, context.surface.framebuffer);
            gl::DeleteFramebuffers(1, &read);
        }
    });
}
