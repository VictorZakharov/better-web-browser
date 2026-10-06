use super::*;

#[test]
fn canvas_native_images_match_scalar_sampling_across_blends_transforms_and_clips() {
    let script = format!(
        "{}\ntestCanvasImagePaint((w,h)=>{{const c=document.createElement('canvas');c.width=w;c.height=h;return c;}});",
        include_str!("../../../../tests/canvas/image-paint.js")
    );
    let (_, outcome) = execute_html(&format!("<script>{script}</script>"));
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
}
