//! Closed context-creation IPC attributes shared by storage and native creation.
use super::ApiVersion;

#[derive(Clone, Copy, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(super) struct Options {
    pub api: ApiVersion,
    pub alpha: bool,
    pub premultiplied_alpha: bool,
    pub depth: bool,
    pub stencil: bool,
    pub antialias: bool,
    pub preserve: bool,
    pub fail_if_major_performance_caveat: bool,
    pub power_preference: PowerPreference,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(super) enum PowerPreference {
    #[default]
    Default,
    LowPower,
    HighPerformance,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            api: ApiVersion::One,
            alpha: true,
            premultiplied_alpha: true,
            depth: true,
            stencil: false,
            antialias: false,
            preserve: false,
            fail_if_major_performance_caveat: false,
            power_preference: PowerPreference::Default,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn omitted_native_attributes_keep_legacy_storage_defaults() {
        let options: Options = serde_json::from_str("{}").unwrap();
        assert_eq!(options.api, ApiVersion::One);
        assert!(options.alpha && options.depth);
        assert!(options.premultiplied_alpha);
        assert!(!options.stencil && !options.antialias && !options.preserve);
        assert!(!options.fail_if_major_performance_caveat);
        assert_eq!(options.power_preference, PowerPreference::Default);
    }

    #[test]
    fn native_bridge_accepts_only_converted_platform_hints() {
        for (encoded, expected) in [
            ("default", PowerPreference::Default),
            ("low-power", PowerPreference::LowPower),
            ("high-performance", PowerPreference::HighPerformance),
        ] {
            let options:Options=serde_json::from_str(&format!(r#"{{"api":"webgl2","power_preference":"{encoded}","fail_if_major_performance_caveat":true}}"#)).unwrap();
            assert_eq!(options.api, ApiVersion::Two);
            assert_eq!(options.power_preference, expected);
            assert!(options.fail_if_major_performance_caveat);
        }
        for invalid in [
            r#"{"power_preference":"software"}"#,
            r#"{"power_preference":"HIGH-PERFORMANCE"}"#,
            r#"{"power_preference":null}"#,
            r#"{"fail_if_major_performance_caveat":"false"}"#,
            r#"{"adapter_luid":42}"#,
            r#"{"backend":"null"}"#,
            r#"{"premultiplied_alpha":"false"}"#,
            r#"{"premultiplied_alpha":null}"#,
        ] {
            assert!(
                serde_json::from_str::<Options>(invalid).is_err(),
                "{invalid}"
            );
        }
    }

    #[test]
    fn native_snapshot_representation_uses_the_converted_alpha_flag() {
        for flag in [false, true] {
            let options: Options =
                serde_json::from_str(&format!(r#"{{"premultiplied_alpha":{flag}}}"#)).unwrap();
            assert_eq!(options.premultiplied_alpha, flag);
        }
    }
}
