/// Live form-control data shared by DOM, script, layout and validity.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ControlState {
    /// Live value for text-like inputs (`None` = pristine, mirror default).
    pub value: Option<String>,
    /// Dirty value flag (input, textarea).
    pub dirty: bool,
    /// True when the last value change was a user edit (length constraints).
    pub user_edited: bool,
    /// Spec user-validity flag, set by user interaction, cleared by reset.
    pub user_validity: bool,
    /// Number inputs: unconvertible user-entered text (`badInput` source).
    pub editing: Option<String>,
    /// Names of selected files. File bytes remain in the realm's File objects;
    /// this browser-owned mirror keeps value, validation and selectors coherent.
    pub file_names: Vec<String>,
    /// Option selectedness.
    pub selectedness: bool,
    /// Option dirtiness (selected-attribute changes stop applying).
    pub selected_dirty: bool,
    /// Output default-value override (`None` = follow descendant text).
    pub default_override: Option<String>,
    /// Custom validity message (newline-normalized on write).
    pub custom_message: String,
    /// Last scripted pattern verdict with the (pattern, values) it was
    /// computed from; selector matching trusts it only on exact deps.
    pub pattern_verdict: Option<(String, Vec<String>, bool)>,
    /// Set by interactive validation reporting; drives visible feedback.
    /// Cleared by any value change or reset.
    pub reported: bool,
    /// Last observed `type` attribute, for type-change transitions.
    pub type_seen: Option<String>,
}
