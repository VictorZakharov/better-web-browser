//! Temporary product identity kept separate from the browser engine.
//!
//! The current name is provisional. Runtime code should consume these values
//! instead of embedding the name so a future rename stays mechanical.

pub const PRODUCT_NAME: &str = "Breeze";
pub const BENCHMARK_ID: &str = "breeze";
pub const USER_AGENT: &str = concat!("Breeze/", env!("CARGO_PKG_VERSION"));
const CHROME_USER_AGENT: &str = concat!(
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 ",
    "(KHTML, like Gecko) Chrome/153.0.0.0 Safari/537.36 Breeze/",
    env!("CARGO_PKG_VERSION")
);
const FIREFOX_USER_AGENT: &str = concat!(
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:156.0) ",
    "Gecko/20100101 Firefox/156.0 Breeze/",
    env!("CARGO_PKG_VERSION")
);

/// Profile-wide, opt-in content negotiation identity. Compatibility modes do
/// not imply that Breeze implements every feature of the named browser.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum UserAgentMode {
    #[default]
    Breeze,
    Chrome,
    Firefox,
}

impl UserAgentMode {
    pub const fn setting(self) -> &'static str {
        match self {
            Self::Breeze => "breeze",
            Self::Chrome => "chrome",
            Self::Firefox => "firefox",
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Breeze => "Breeze (default)",
            Self::Chrome => "Chrome compatible",
            Self::Firefox => "Firefox compatible",
        }
    }

    pub const fn user_agent(self) -> &'static str {
        match self {
            Self::Breeze => USER_AGENT,
            Self::Chrome => CHROME_USER_AGENT,
            Self::Firefox => FIREFOX_USER_AGENT,
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "breeze" => Some(Self::Breeze),
            "chrome" => Some(Self::Chrome),
            "firefox" => Some(Self::Firefox),
            _ => None,
        }
    }
}

static RENDERER_MODE: std::sync::OnceLock<UserAgentMode> = std::sync::OnceLock::new();

/// Called once during the isolated renderer bootstrap, before any page realm.
pub fn install_renderer_user_agent_mode(mode: UserAgentMode) -> Result<(), &'static str> {
    RENDERER_MODE
        .set(mode)
        .map_err(|_| "renderer User-Agent mode was already initialized")
}

pub fn renderer_user_agent() -> &'static str {
    RENDERER_MODE
        .get()
        .copied()
        .unwrap_or_default()
        .user_agent()
}

pub const HOME_URL: &str = "https://browser.local/";

pub const HOME_HTML: &str = r#"
    <title>Breeze</title>
    <style>
        body { margin: 0; background: #f8fafc; color: #172033; font: 16px/1.55 "Segoe UI", sans-serif; }
        main { max-width: 820px; margin: 72px auto; padding: 44px 52px; background: white; border: 1px solid #dce3ec; border-radius: 18px; }
        h1 { margin: 0 0 12px; color: #162238; font-size: 38px; }
        h2 { margin-top: 32px; color: #243451; }
        a { color: #1558d6; text-decoration: none; }
        li { margin: 7px 0; }
    </style>
    <main>
        <h1>Breeze</h1>
        <p>A performance-first browser engine written in Rust. Enter an address or search above.</p>
        <h2>Engine milestone</h2>
        <ul>
            <li>Standards-oriented HTML5 DOM construction</li>
            <li>Owned CSS cascade, box layout, display list, images, and forms</li>
            <li>Native HTTPS, history, scrolling, and process telemetry</li>
        </ul>
        <p>Try <a href="https://www.google.com/">Google</a> or <a href="https://example.org/">example.org</a>.</p>
    </main>
"#;

#[cfg(test)]
mod user_agent_tests {
    use super::*;

    #[test]
    fn identities_remain_distinct_and_attributable() {
        assert_eq!(UserAgentMode::default(), UserAgentMode::Breeze);
        assert_eq!(UserAgentMode::Breeze.user_agent(), USER_AGENT);
        assert!(UserAgentMode::Chrome.user_agent().contains("Chrome/153."));
        assert!(UserAgentMode::Firefox.user_agent().contains("Firefox/156."));
        for mode in [
            UserAgentMode::Breeze,
            UserAgentMode::Chrome,
            UserAgentMode::Firefox,
        ] {
            assert_eq!(UserAgentMode::parse(mode.setting()), Some(mode));
            assert!(mode.user_agent().contains("Breeze/"));
        }
        assert_eq!(UserAgentMode::parse("unknown"), None);
    }
}
