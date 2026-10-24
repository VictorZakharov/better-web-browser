//! The bootstrap wire list and the native WebGL void-command policy must agree.
use crate::engine::webgl::{MAX_NUMERIC_VALUES, NumericCommand};

#[test]
fn every_bootstrap_numeric_operation_is_admitted_by_the_native_void_policy() {
    let source = include_str!("../../bootstrap/webgl_private_wire.js");
    let list = source
        .split_once("const webGlWireNumericOperations = new Set((")
        .unwrap()
        .1
        .split_once(".split(' '));")
        .unwrap()
        .0;
    let mut count = 0;
    for fragment in list
        .split('\'')
        .enumerate()
        .filter_map(|(index, text)| (index % 2 == 1).then_some(text))
    {
        for op in fragment.split_whitespace() {
            assert!(
                NumericCommand::new(op.into(), vec![], vec![]).is_some(),
                "{op}"
            );
            count += 1;
        }
    }
    assert!(
        count > 100,
        "operation declarations must not silently disappear"
    );
    assert_eq!(MAX_NUMERIC_VALUES, 64);
}
