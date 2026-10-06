use super::*;

#[test]
fn native_whole_surface_compositing_matches_scalar_and_clipped_shadow_contracts() {
    let source = format!(
        "{}\ntestCanvasCompositeLayers((width,height)=>{{const c=document.createElement('canvas');c.width=width;c.height=height;return c;}});",
        include_str!("../../../../tests/canvas/composite-layers.js")
    );
    let (_, outcome) = execute_html(&format!("<script>{source}</script>"));
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
}
