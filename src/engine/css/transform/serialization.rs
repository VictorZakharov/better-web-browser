use super::*;

pub(in crate::engine::css) fn serialize_transform(transform: &TransformList) -> String {
    if transform.is_none() {
        return "none".into();
    }
    transform
        .0
        .iter()
        .map(|operation| match operation {
            Operation::Translate(v) => format!(
                "translate({}, {})",
                serialize_length(v.x.clone()),
                serialize_length(v.y.clone())
            ),
            Operation::Scale([x, y]) => format!("scale({x}, {y})"),
            Operation::Rotate(v) => format!("rotate({}rad)", v),
            Operation::Skew([x, y]) => format!("skew({x}rad, {y}rad)"),
            Operation::Matrix(v) => format!(
                "matrix({}, {}, {}, {}, {}, {})",
                v.0[0], v.0[1], v.0[2], v.0[3], v.0[4], v.0[5]
            ),
        })
        .collect::<Vec<_>>()
        .join(" ")
}
