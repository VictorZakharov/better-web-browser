//! Closed list: arbitrary attributes must not receive a local-selector proof.
pub(crate) fn is_local_graphics_attribute(name: &str) -> bool {
    [
        "d",
        "points",
        "x",
        "y",
        "x1",
        "x2",
        "y1",
        "y2",
        "cx",
        "cy",
        "r",
        "rx",
        "ry",
        "transform",
    ]
    .iter()
    .any(|known| name.eq_ignore_ascii_case(known))
}
