use super::{PROHIBIT_DYNAMIC_CODE, renderer_mitigations};

#[test]
fn jit_probe_changes_only_the_dynamic_code_policy() {
    let production = renderer_mitigations(true);
    let jit_probe = renderer_mitigations(false);

    assert_eq!(production & PROHIBIT_DYNAMIC_CODE, PROHIBIT_DYNAMIC_CODE);
    assert_eq!(jit_probe & PROHIBIT_DYNAMIC_CODE, 0);
    assert_eq!(
        production & !PROHIBIT_DYNAMIC_CODE,
        jit_probe & !PROHIBIT_DYNAMIC_CODE
    );
}
