use super::{PROHIBIT_DYNAMIC_CODE, renderer_mitigations};

#[test]
fn production_v8_policy_permits_jit_without_dropping_other_mitigations() {
    let production = renderer_mitigations();
    let retained = 0x1
        | 0x4
        | (0x3 << 8)
        | (0x1 << 12)
        | (0x1 << 16)
        | (0x1 << 20)
        | (0x1 << 24)
        | (0x1 << 32)
        | (0x1 << 40)
        | (0x1 << 52)
        | (0x1 << 56);

    assert_eq!(production & PROHIBIT_DYNAMIC_CODE, 0);
    assert_eq!(production, retained);
}
