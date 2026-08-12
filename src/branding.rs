//! Temporary product identity kept separate from the browser engine.
//!
//! The current name is provisional. Runtime code should consume these values
//! instead of embedding the name so a future rename stays mechanical.

pub const PRODUCT_NAME: &str = "Breeze";
pub const BENCHMARK_ID: &str = "breeze";
pub const USER_AGENT: &str = concat!(
    "Breeze/",
    env!("CARGO_PKG_VERSION"),
    " (+https://localhost)"
);
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
