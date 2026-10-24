//! Native mip generation follows WebGL2 dimensions and preserves storage budgets.
use super::api_version_tests::{call, version_two};
use super::core_uniform_tests::{VERTEX, program};
use super::*;

fn image(context: &mut WebGl, width: i64, height: i64) {
    let bytes: Vec<_> = (0..width * height).flat_map(|_| [255, 0, 0, 255]).collect();
    let command = Command {
        op: "texImage2D".into(),
        i: vec![
            gl::TEXTURE_2D as i64,
            0,
            0x8058,
            width,
            height,
            0,
            gl::RGBA as i64,
            gl::UNSIGNED_BYTE as i64,
        ],
        f: vec![],
        text: String::new(),
    };
    assert_eq!(context.dispatch(&command, Some(&bytes)), Ok(Value::Null));
}

fn texture(context: &mut WebGl) -> u32 {
    let id = call(context, "createTexture", &[], "").as_u64().unwrap() as u32;
    call(
        context,
        "bindTexture",
        &[gl::TEXTURE_2D as i64, id as i64],
        "",
    );
    call(
        context,
        "texParameteri",
        &[
            gl::TEXTURE_2D as i64,
            gl::TEXTURE_MIN_FILTER as i64,
            gl::NEAREST_MIPMAP_NEAREST as i64,
        ],
        "",
    );
    call(
        context,
        "texParameteri",
        &[
            gl::TEXTURE_2D as i64,
            gl::TEXTURE_MAG_FILTER as i64,
            gl::NEAREST as i64,
        ],
        "",
    );
    id
}

#[test]
fn webgl2_npot_generated_mipmaps_sample_pixels_and_repeated_generation_is_not_recharged() {
    session::run_native_test(|| {
        let mut context = version_two();
        let id = texture(&mut context);
        image(&mut context, 3, 5);
        let before = context.resource_bytes;
        call(&mut context, "generateMipmap", &[gl::TEXTURE_2D as i64], "");
        // Existing lifetime accounting reserves twice the native allocation.
        assert_eq!(context.resource_bytes - before, (2 + 1) * 4 * 2);
        let images = &context.objects.get(id, Kind::Texture).unwrap().core_images;
        assert_eq!(images[&(gl::TEXTURE_2D, 1)].width, 1);
        assert_eq!(images[&(gl::TEXTURE_2D, 1)].height, 2);
        assert_eq!(images[&(gl::TEXTURE_2D, 2)].height, 1);
        let charged = context.resource_bytes;
        for _ in 0..3 {
            call(&mut context, "generateMipmap", &[gl::TEXTURE_2D as i64], "");
        }
        assert_eq!(context.resource_bytes, charged);
        program(
            &mut context,
            VERTEX,
            "#version 300 es\nprecision highp float;uniform sampler2D t;out vec4 color;void main(){color=textureLod(t,vec2(0.5),2.0);}",
        );
        call(
            &mut context,
            "drawArrays",
            &[gl::TRIANGLES as i64, 0, 3],
            "",
        );
        assert!(
            context
                .surface
                .snapshot()
                .unwrap()
                .chunks_exact(4)
                .all(|p| p == [255, 0, 0, 255])
        );
        assert_eq!(call(&mut context, "getError", &[], ""), json!(0));
    });
}

#[test]
fn webgl2_regenerated_larger_mips_charge_the_growth_of_existing_levels() {
    session::run_native_test(|| {
        let mut context = version_two();
        texture(&mut context);
        image(&mut context, 2, 2);
        call(&mut context, "generateMipmap", &[gl::TEXTURE_2D as i64], "");
        image(&mut context, 8, 8);
        let before = context.resource_bytes;
        call(&mut context, "generateMipmap", &[gl::TEXTURE_2D as i64], "");
        assert_eq!(context.resource_bytes - before, (16 - 1 + 4 + 1) * 4 * 2);
        assert_eq!(call(&mut context, "getError", &[], ""), json!(0));
    });
}

#[test]
fn webgl2_immutable_mip_generation_and_fractional_lod_queries_preserve_state() {
    session::run_native_test(|| {
        let mut context = version_two();
        texture(&mut context);
        call(
            &mut context,
            "texStorage2D",
            &[gl::TEXTURE_2D as i64, 3, 0x8058, 4, 4],
            "",
        );
        let before = context.resource_bytes;
        call(&mut context, "generateMipmap", &[gl::TEXTURE_2D as i64], "");
        assert_eq!(context.resource_bytes, before);
        let command = Command {
            op: "texParameterf".into(),
            i: vec![gl::TEXTURE_2D as i64, 0x813a],
            f: vec![0.25],
            text: String::new(),
        };
        assert_eq!(context.dispatch(&command, None), Ok(Value::Null));
        assert_eq!(
            call(
                &mut context,
                "getTexParameter",
                &[gl::TEXTURE_2D as i64, 0x813a],
                ""
            ),
            json!(0.25)
        );
        assert_eq!(call(&mut context, "getError", &[], ""), json!(0));
    });
}
