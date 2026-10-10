//! Draw disjoint unscaled source rectangles; skipped bytes are all zero.
use super::*;
pub(in crate::windows_app) unsafe fn paint(
    destination: Hdc,
    bitmap: Hbitmap,
    bounds: &Rect,
    coverage: &[Rect],
) -> bool {
    if coverage.is_empty() {
        return true;
    }
    let source = CreateCompatibleDC(destination);
    if source.is_null() {
        return false;
    }
    let previous = SelectObject(source, bitmap);
    if previous.is_null() || previous as usize == usize::MAX {
        DeleteDC(source);
        return false;
    }
    for rect in coverage {
        AlphaBlend(
            destination,
            bounds.left + rect.left,
            bounds.top + rect.top,
            rect.width(),
            rect.height(),
            source,
            rect.left,
            rect.top,
            rect.width(),
            rect.height(),
            BlendFunction {
                operation: 0,
                flags: 0,
                source_constant_alpha: 255,
                alpha_format: 1,
            },
        );
    }
    // A partially completed source-over draw cannot be retried as a whole: doing
    // so would blend successful translucent segments twice. Admission validates
    // all rectangles before painting; native failures retain the usual best-effort path.
    SelectObject(source, previous);
    DeleteDC(source);
    true
}
