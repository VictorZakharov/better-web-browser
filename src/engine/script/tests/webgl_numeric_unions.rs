use super::webgl_instancing::check;

#[test]
fn webgl_numeric_union_brands_detachment_and_conversion_order() {
    check(&format!(
        "{}\ntestNumericUnions(()=>document.createElement('canvas'));document.querySelector('output').textContent='pass';",
        include_str!("../../../../tests/webgl/numeric-unions.js")
    ));
}
