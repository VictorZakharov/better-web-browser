//! Active sandbox flags are inherited and snapshotted at Document creation.
#[derive(Clone, Copy, Default)]
pub(in crate::engine::script) struct Sandbox {
    pub sandboxed: bool,
    pub opaque_origin: bool,
    pub scripts_blocked: bool,
    pub forms_blocked: bool,
    pub top_navigation: bool,
    pub top_activation: bool,
}

impl Sandbox {
    pub(in crate::engine::script) fn child(self, flags: Option<&str>) -> Self {
        let Some(flags) = flags else { return self };
        let allows = |name: &str| {
            flags
                .split_ascii_whitespace()
                .any(|flag| flag.eq_ignore_ascii_case(name))
        };
        Self {
            sandboxed: true,
            opaque_origin: self.opaque_origin || !allows("allow-same-origin"),
            scripts_blocked: self.scripts_blocked || !allows("allow-scripts"),
            forms_blocked: self.forms_blocked || !allows("allow-forms"),
            top_navigation: (!self.sandboxed || self.top_navigation)
                && allows("allow-top-navigation"),
            top_activation: (!self.sandboxed || self.top_navigation || self.top_activation)
                && (allows("allow-top-navigation")
                    || allows("allow-top-navigation-by-user-activation")),
        }
    }
}
