//! Tiny project-owned images, encoded by the same existing OSS encoder packages
//! already present in Cargo.lock. These helpers are never part of the browser.
use image::{DynamicImage, ImageFormat, RgbaImage};
use std::io::Cursor;

pub fn rgba_png() -> Vec<u8> {
    let mut bytes = Cursor::new(Vec::new());
    let image = RgbaImage::from_raw(2, 1, vec![10, 20, 30, 255, 40, 50, 60, 128]).unwrap();
    DynamicImage::ImageRgba8(image)
        .write_to(&mut bytes, ImageFormat::Png)
        .unwrap();
    bytes.into_inner()
}

pub fn oriented_jpeg(orientation: u16) -> Vec<u8> {
    use image::ImageEncoder;
    let mut jpeg = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 100)
        .write_image(
            &[20, 40, 60, 200, 100, 50],
            2,
            1,
            image::ColorType::Rgb8.into(),
        )
        .unwrap();
    // Original minimal TIFF IFD: Orientation is one SHORT; next IFD is absent.
    let mut exif = b"Exif\0\0II\x2a\0\x08\0\0\0\x01\0\x12\x01\x03\0\x01\0\0\0".to_vec();
    exif.extend_from_slice(&orientation.to_le_bytes());
    exif.extend_from_slice(&[0; 6]);
    jpeg_segment(&jpeg, 0xe1, &exif)
}

pub fn jpeg_segment(jpeg: &[u8], marker: u8, payload: &[u8]) -> Vec<u8> {
    let mut bytes = jpeg[..2].to_vec();
    bytes.extend_from_slice(&[0xff, marker]);
    bytes.extend_from_slice(&((payload.len() + 2) as u16).to_be_bytes());
    bytes.extend_from_slice(payload);
    bytes.extend_from_slice(&jpeg[2..]);
    bytes
}

pub fn disposing_gif() -> Vec<u8> {
    let mut bytes = Vec::new();
    {
        let palette = [0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255];
        let mut encoder = gif::Encoder::new(&mut bytes, 2, 2, &palette).unwrap();
        encoder.set_repeat(gif::Repeat::Finite(2)).unwrap();
        let base = gif::Frame {
            width: 2,
            height: 2,
            buffer: vec![1; 4].into(),
            delay: 4,
            dispose: gif::DisposalMethod::Keep,
            ..Default::default()
        };
        encoder.write_frame(&base).unwrap();
        let mut overlay = gif::Frame {
            width: 1,
            height: 1,
            left: 1,
            buffer: vec![2].into(),
            delay: 5,
            dispose: gif::DisposalMethod::Previous,
            ..Default::default()
        };
        encoder.write_frame(&overlay).unwrap();
        overlay.left = 0;
        overlay.top = 1;
        overlay.buffer = vec![3].into();
        overlay.delay = 6;
        overlay.dispose = gif::DisposalMethod::Keep;
        encoder.write_frame(&overlay).unwrap();
    }
    bytes
}

pub fn apng(poster: bool) -> Vec<u8> {
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, 2, 2);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.set_animated(3, 2).unwrap();
        encoder.set_sep_def_img(poster).unwrap();
        let mut writer = encoder.write_header().unwrap();
        if poster {
            writer
                .write_image_data(&[90, 80, 70, 255].repeat(4))
                .unwrap();
        }
        writer.set_frame_delay(1, 25).unwrap();
        writer.set_blend_op(png::BlendOp::Source).unwrap();
        writer.set_dispose_op(png::DisposeOp::None).unwrap();
        writer
            .write_image_data(&[255, 0, 0, 255].repeat(4))
            .unwrap();
        writer.set_frame_dimension(1, 1).unwrap();
        writer.set_frame_position(1, 0).unwrap();
        writer.set_blend_op(png::BlendOp::Over).unwrap();
        writer.set_dispose_op(png::DisposeOp::Previous).unwrap();
        writer.write_image_data(&[0, 255, 0, 255]).unwrap();
        writer.set_frame_position(0, 1).unwrap();
        writer.set_dispose_op(png::DisposeOp::None).unwrap();
        writer.write_image_data(&[0, 0, 255, 255]).unwrap();
        writer.finish().unwrap();
    }
    bytes
}

fn chunk(output: &mut Vec<u8>, name: &[u8; 4], payload: &[u8]) {
    output.extend_from_slice(name);
    output.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    output.extend_from_slice(payload);
    if !payload.len().is_multiple_of(2) {
        output.push(0);
    }
}

fn u24(output: &mut Vec<u8>, value: u32) {
    output.extend_from_slice(&value.to_le_bytes()[..3]);
}

pub fn animated_webp() -> Vec<u8> {
    // Original fixture container, not a codec implementation. See the official
    // WebP RIFF specification: VP8X, ANIM and ANMF chunk layouts. Each lossless
    // frame's VP8L payload is supplied by image's existing WebP encoder.
    let mut body = b"WEBP".to_vec();
    let mut extended = vec![2, 0, 0, 0];
    u24(&mut extended, 1);
    u24(&mut extended, 1);
    chunk(&mut body, b"VP8X", &extended);
    chunk(&mut body, b"ANIM", &[0, 0, 0, 0, 2, 0]);
    for color in [[255, 0, 0, 255], [0, 255, 0, 255], [0, 0, 255, 255]] {
        let image = RgbaImage::from_raw(2, 2, color.repeat(4)).unwrap();
        let mut encoded = Cursor::new(Vec::new());
        DynamicImage::ImageRgba8(image)
            .write_to(&mut encoded, ImageFormat::WebP)
            .unwrap();
        let mut frame = Vec::new();
        for dimension in [0, 0, 1, 1, 40] {
            u24(&mut frame, dimension);
        }
        frame.push(2); // No blending, retain canvas after display.
        frame.extend_from_slice(&encoded.into_inner()[12..]);
        chunk(&mut body, b"ANMF", &frame);
    }
    let mut output = b"RIFF".to_vec();
    output.extend_from_slice(&(body.len() as u32).to_le_bytes());
    output.extend_from_slice(&body);
    output
}

pub fn all() -> Vec<(&'static str, &'static str, Vec<u8>)> {
    vec![
        ("rgba.png", "image/png", rgba_png()),
        ("disposal.gif", "image/gif", disposing_gif()),
        ("composition.png", "image/png", apng(false)),
        ("poster.png", "image/png", apng(true)),
        ("animation.webp", "image/webp", animated_webp()),
        ("oriented.jpg", "image/jpeg", oriented_jpeg(6)),
    ]
}
