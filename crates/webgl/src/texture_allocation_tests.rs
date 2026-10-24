//! Texture redefinition must not exhaust the bounded context through duplicate charges.
use super::api_version_tests::{call, version_two};
use super::compressed_texture_tests::{context, texture, upload};
use super::*;

fn image(context: &mut WebGl, width: i64, height: i64) -> Result<Value> {
    upload(
        context,
        "texImage2D",
        &[
            gl::TEXTURE_2D as i64,
            0,
            gl::RGBA as i64,
            width,
            height,
            0,
            gl::RGBA as i64,
            gl::UNSIGNED_BYTE as i64,
        ],
        None,
    )
}

#[test]
fn texture_redefinitions_charge_only_lifetime_high_water_growth() {
    session::run_native_test(|| {
        for factory in [context as fn() -> WebGl, version_two] {
            let mut context = factory();
            let id = texture(&mut context, gl::TEXTURE_2D);
            let baseline = context.resource_bytes;
            for _ in 0..100 {
                image(&mut context, 8, 8).unwrap();
            }
            assert_eq!(context.resource_bytes, baseline + 8 * 8 * 4 * 2);
            for dimension in [4, 2, 8, 1, 8] {
                image(&mut context, dimension, dimension).unwrap();
            }
            assert_eq!(context.resource_bytes, baseline + 8 * 8 * 4 * 2);
            image(&mut context, 16, 16).unwrap();
            assert_eq!(context.resource_bytes, baseline + 16 * 16 * 4 * 2);
            let object = context.objects.get(id, Kind::Texture).unwrap();
            assert_eq!(object.capacity, 16 * 16 * 4);
            assert_eq!(object.texture_allocations.len(), 1);
        }
    });
}

#[test]
fn budget_failure_and_invalid_dimensions_preserve_texture_definition() {
    session::run_native_test(|| {
        let mut context = context();
        let id = texture(&mut context, gl::TEXTURE_2D);
        image(&mut context, 4, 4).unwrap();
        let charged = context.resource_bytes;
        context.resource_limit = charged + 1;
        assert_eq!(image(&mut context, 8, 8), Err(gl::OUT_OF_MEMORY));
        assert_eq!(image(&mut context, -1, 4), Err(gl::INVALID_VALUE));
        assert_eq!(context.resource_bytes, charged);
        assert_eq!(
            context.objects.get(id, Kind::Texture).unwrap().core_images[&(gl::TEXTURE_2D, 0)].width,
            4
        );
        image(&mut context, 4, 4).unwrap();
    });
}

#[test]
fn mip_regeneration_and_base_shrinking_do_not_recharge_generated_images() {
    session::run_native_test(|| {
        for factory in [context as fn() -> WebGl, version_two] {
            let mut context = factory();
            let id = texture(&mut context, gl::TEXTURE_2D);
            let baseline = context.resource_bytes;
            image(&mut context, 8, 8).unwrap();
            for _ in 0..50 {
                call(&mut context, "generateMipmap", &[gl::TEXTURE_2D as i64], "");
            }
            let charged = baseline + (64 + 16 + 4 + 1) * 4 * 2;
            assert_eq!(context.resource_bytes, charged);
            image(&mut context, 4, 4).unwrap();
            call(&mut context, "generateMipmap", &[gl::TEXTURE_2D as i64], "");
            image(&mut context, 8, 8).unwrap();
            call(&mut context, "generateMipmap", &[gl::TEXTURE_2D as i64], "");
            assert_eq!(context.resource_bytes, charged);
            assert_eq!(
                context
                    .objects
                    .get(id, Kind::Texture)
                    .unwrap()
                    .texture_allocations
                    .len(),
                4
            );
        }
    });
}

#[test]
fn reservation_validation_is_atomic_and_counts_distinct_images() {
    session::run_native_test(|| {
        let mut context = context();
        let id = texture(&mut context, gl::TEXTURE_CUBE_MAP);
        let baseline = context.resource_bytes;
        assert!(matches!(
            context.prepare_texture_storage(id, vec![((0x8515, 0), 4), ((0x8515, 0), 8)]),
            Err(gl::INVALID_OPERATION)
        ));
        assert!(matches!(
            context.prepare_texture_storage(id, vec![((0x8515, 0), usize::MAX)]),
            Err(gl::OUT_OF_MEMORY)
        ));
        assert_eq!(context.resource_bytes, baseline);
        let reservation = context
            .prepare_texture_storage(
                id,
                vec![((0x8515, 0), 4), ((0x8516, 0), 8), ((0x8515, 1), 2)],
            )
            .unwrap();
        assert_eq!(context.resource_bytes, baseline);
        context.commit_texture_storage(reservation).unwrap();
        assert_eq!(context.resource_bytes, baseline + 28);
        assert_eq!(context.objects.get(id, Kind::Texture).unwrap().capacity, 14);
    });
}

#[test]
fn volume_redefinition_and_immutable_conversion_share_per_level_budget() {
    session::run_native_test(|| {
        let mut context = version_two();
        for target in [0x806f, 0x8c1a] {
            let id = texture(&mut context, target);
            let baseline = context.resource_bytes;
            for _ in 0..25 {
                upload(
                    &mut context,
                    "texImage3D",
                    &[
                        target as i64,
                        0,
                        0x8058,
                        4,
                        4,
                        4,
                        0,
                        gl::RGBA as i64,
                        gl::UNSIGNED_BYTE as i64,
                    ],
                    None,
                )
                .unwrap();
            }
            assert_eq!(context.resource_bytes, baseline + 4 * 4 * 4 * 4 * 2);
            call(
                &mut context,
                "texStorage3D",
                &[target as i64, 3, 0x8058, 4, 4, 4],
                "",
            );
            let mip_texels = if target == 0x806f { 8 + 1 } else { 16 + 4 };
            assert_eq!(context.resource_bytes, baseline + (64 + mip_texels) * 4 * 2);
            assert_eq!(
                context
                    .objects
                    .get(id, Kind::Texture)
                    .unwrap()
                    .texture_allocations
                    .len(),
                3
            );
            let charged = context.resource_bytes;
            assert_eq!(
                upload(
                    &mut context,
                    "texStorage3D",
                    &[target as i64, 3, 0x8058, 4, 4, 4],
                    None
                ),
                Err(gl::INVALID_OPERATION)
            );
            assert_eq!(context.resource_bytes, charged);
        }
    });
}

#[test]
fn compressed_redefinitions_and_immutable_storage_keep_expanded_high_water() {
    use super::compressed_capabilities::Family;
    use super::compressed_texture_tests::{block, enable};
    session::run_native_test(|| {
        let mut context = version_two();
        for family in Family::ALL {
            enable(&mut context, family);
            for format in family.formats() {
                let id = texture(&mut context, gl::TEXTURE_2D);
                let baseline = context.resource_bytes;
                let block = block(format, 0xf800);
                for _ in 0..20 {
                    upload(
                        &mut context,
                        "compressedTexImage2D",
                        &[gl::TEXTURE_2D as i64, 0, format as i64, 4, 4, 0],
                        Some(&block),
                    )
                    .unwrap();
                }
                assert_eq!(context.resource_bytes, baseline + 4 * 4 * 4 * 2);
                call(
                    &mut context,
                    "texStorage2D",
                    &[gl::TEXTURE_2D as i64, 3, format as i64, 4, 4],
                    "",
                );
                assert_eq!(context.resource_bytes, baseline + (16 + 4 + 1) * 4 * 2);
                assert_eq!(
                    context
                        .objects
                        .get(id, Kind::Texture)
                        .unwrap()
                        .texture_allocations
                        .len(),
                    3
                );
            }
        }
    });
}

#[test]
fn copying_and_uploading_the_same_image_share_one_reservation() {
    session::run_native_test(|| {
        let mut context = context();
        texture(&mut context, gl::TEXTURE_2D);
        image(&mut context, 4, 4).unwrap();
        let charged = context.resource_bytes;
        for _ in 0..25 {
            call(
                &mut context,
                "copyTexImage2D",
                &[gl::TEXTURE_2D as i64, 0, gl::RGBA as i64, 0, 0, 4, 4, 0],
                "",
            );
            image(&mut context, 4, 4).unwrap();
        }
        assert_eq!(context.resource_bytes, charged);
        call(&mut context, "generateMipmap", &[gl::TEXTURE_2D as i64], "");
        assert_eq!(context.resource_bytes, charged + (4 + 1) * 4 * 2);
    });
}

#[test]
fn invalid_native_cube_generation_and_zero_size_do_not_charge_mips() {
    session::run_native_test(|| {
        let mut context = context();
        texture(&mut context, gl::TEXTURE_CUBE_MAP);
        for face in 0x8515..=0x851a {
            let size = if face == 0x851a { 2 } else { 4 };
            upload(
                &mut context,
                "texImage2D",
                &[
                    face,
                    0,
                    gl::RGBA as i64,
                    size,
                    size,
                    0,
                    gl::RGBA as i64,
                    gl::UNSIGNED_BYTE as i64,
                ],
                None,
            )
            .unwrap();
        }
        let charged = context.resource_bytes;
        assert_eq!(
            upload(
                &mut context,
                "generateMipmap",
                &[gl::TEXTURE_CUBE_MAP as i64],
                None
            ),
            Err(gl::INVALID_OPERATION)
        );
        assert_eq!(context.resource_bytes, charged);
        texture(&mut context, gl::TEXTURE_2D);
        image(&mut context, 0, 0).unwrap();
        assert_eq!(
            upload(
                &mut context,
                "generateMipmap",
                &[gl::TEXTURE_2D as i64],
                None
            ),
            Err(gl::INVALID_OPERATION)
        );
        assert_eq!(context.resource_bytes, charged);
    });
}

#[test]
fn deleted_texture_names_reclaim_only_after_retained_framebuffer_is_retired() {
    session::run_native_test(|| {
        let mut context = context();
        let baseline = context.resource_bytes;
        let id = texture(&mut context, gl::TEXTURE_2D);
        image(&mut context, 4, 4).unwrap();
        let framebuffer = call(&mut context, "createFramebuffer", &[], "")
            .as_i64()
            .unwrap();
        call(
            &mut context,
            "bindFramebuffer",
            &[gl::FRAMEBUFFER as i64, framebuffer],
            "",
        );
        call(
            &mut context,
            "framebufferTexture2D",
            &[
                gl::FRAMEBUFFER as i64,
                gl::COLOR_ATTACHMENT0 as i64,
                gl::TEXTURE_2D as i64,
                id as i64,
                0,
            ],
            "",
        );
        call(
            &mut context,
            "bindFramebuffer",
            &[gl::FRAMEBUFFER as i64, 0],
            "",
        );
        let charged = context.resource_bytes;
        call(&mut context, "deleteTexture", &[id as i64], "");
        assert_eq!(context.resource_bytes, charged);
        let replacement = texture(&mut context, gl::TEXTURE_2D);
        image(&mut context, 4, 4).unwrap();
        assert_eq!(context.resource_bytes, charged + 4 * 4 * 4 * 2);
        call(&mut context, "deleteFramebuffer", &[framebuffer], "");
        assert_eq!(context.resource_bytes, charged);
        call(&mut context, "deleteTexture", &[replacement as i64], "");
        assert_eq!(context.resource_bytes, baseline);
    });
}
