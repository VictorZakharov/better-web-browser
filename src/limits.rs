//! Central hostile-input and process budgets.
//!
//! These are security boundaries, not compatibility targets. Inputs that exceed a byte or count
//! budget fail closed or produce a deliberately truncated document; renderer process budgets are
//! enforced by the broker and Windows Job Object. See `docs/security-and-fuzzing.md`.

use std::time::Duration;

pub const MAX_HTML_INPUT_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_DOM_NODES: usize = 100_000;
pub const MAX_DOM_DEPTH: usize = 512;
pub const MAX_HTML_PARSE_ERRORS: usize = 256;
pub const MAX_RENDERED_TEXT_BYTES: usize = 2 * 1024 * 1024;

pub const MAX_CSS_SOURCE_BYTES: usize = 2 * 1024 * 1024;
pub const MAX_CSS_RULES: usize = 20_000;
pub const MAX_CSS_NESTING_DEPTH: usize = 64;
pub const MAX_CSS_DECLARATIONS_PER_RULE: usize = 256;

pub const MAX_URL_BYTES: usize = 16 * 1024;
pub const MAX_RESPONSE_BODY_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_PREFLIGHT_BODY_BYTES: usize = 64 * 1024;
pub const MAX_REDIRECTS: usize = 20;
pub const PAGE_RESOURCE_BUDGET: u64 = 32 * 1024 * 1024;

pub const MAX_COOKIES: usize = 3_000;
pub const MAX_COOKIES_PER_DOMAIN: usize = 180;
pub const MAX_COOKIE_HEADER_BYTES: usize = 64 * 1024;
pub const MAX_COOKIE_ASSIGNMENT_BYTES: usize = 4 * 1024;
pub const MAX_PERSISTED_COOKIE_BYTES: usize = 16 * 1024 * 1024;

pub const MAX_STORAGE_ORIGINS: usize = 256;
pub const MAX_STORAGE_ENTRIES_PER_ORIGIN: usize = 1_024;
pub const MAX_STORAGE_KEY_BYTES: usize = 4 * 1024;
pub const MAX_STORAGE_VALUE_BYTES: usize = 192 * 1024;
pub const MAX_STORAGE_BYTES_PER_ORIGIN: usize = 5 * 1024 * 1024;
pub const MAX_PERSISTED_STORAGE_BYTES: usize = 64 * 1024 * 1024;

pub const MAX_SCRIPT_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_DOCUMENT_WRITE_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_DYNAMIC_SCRIPTS: usize = 32;
pub const MAX_DOM_MUTATIONS_PER_TASK: usize = if cfg!(test) { 256 } else { 10_000 };
pub const MAX_POST_LOAD_TIMER_CALLBACKS: usize = 128;
pub const MAX_SCRIPT_LOOP_ITERATIONS: u64 = if cfg!(test) { 25_000 } else { 5_000_000 };

pub const MAX_DECODED_IMAGE_PIXELS: u64 = 32 * 1024 * 1024;
pub const MAX_DECODED_IMAGE_BYTES: u64 = MAX_DECODED_IMAGE_PIXELS * 4;
pub const MAX_DECODED_IMAGE_DIMENSION: u32 = 32 * 1024;
pub const MAX_IMAGE_SOURCE_BYTES: usize = MAX_RESPONSE_BODY_BYTES;
pub const MAX_SVG_SOURCE_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_EMBEDDED_IMAGE_URL_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_PAGE_IMAGES: usize = 64;
pub const MAX_STYLE_IMAGES: usize = 64;
pub const MAX_INLINE_SVGS: usize = 64;
pub const MAX_WEB_FONTS: usize = 16;
pub const MAX_FONT_BYTES: usize = 32 * 1024 * 1024;
pub const MAX_FONT_TABLES: usize = 256;
pub const MAX_GLYPH_RASTERS: usize = 65_536;
pub const MAX_GLYPH_RASTER_DIMENSION: u32 = 1_024;
pub const MAX_GLYPH_RASTER_PIXELS: u64 = 1024 * 1024;
pub const MAX_GLYPH_RASTER_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_PRESENTED_GLYPH_BYTES: usize = 48 * 1024 * 1024;
pub const MAX_STYLESHEETS: usize = 16;
pub const MAX_PAGE_SCRIPTS: usize = 64;

pub const MAX_CONTROL_PAYLOAD: usize = 256 * 1024;
pub const MAX_FRAME_PAYLOAD: usize = 8 * 1024 * 1024;
pub const MAX_FETCH_STREAM_CHUNK_BYTES: usize = 64 * 1024;
pub const MAX_QUEUED_FETCH_STREAM_CHUNKS: usize = 8;
pub const MAX_RENDERER_FETCH_REQUESTS_PER_BATCH: usize = 256;
pub const MAX_PARALLEL_RENDERER_FETCHES: usize = 8;
pub const MAX_RENDERER_FETCH_BATCH_BODY_BYTES: usize = 32 * 1024 * 1024;
pub const MAX_RENDERER_FETCH_BATCH_METADATA_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_RENDERER_FETCH_HEADERS: usize = 256;
pub const MAX_FETCH_HEADER_NAME_BYTES: usize = 1024;
pub const MAX_FETCH_HEADER_VALUE_BYTES: usize = 16 * 1024;
pub const MAX_QUEUED_BROWSER_COMMANDS: usize = 8;
/// Valid native input retained per tab while the renderer command pipe applies backpressure.
/// This is deliberately no larger than the broker command channel so a stalled renderer cannot
/// make browser-owned memory grow with the rate of Windows input messages.
pub const MAX_PENDING_RENDERER_INPUTS: usize = MAX_QUEUED_BROWSER_COMMANDS;
pub const MAX_QUEUED_RENDERER_IPC_MESSAGES: usize = 8;
pub const MAX_QUEUED_RENDERER_EVENTS: usize = 256;
pub const MAX_QUEUED_RENDERER_FETCH_BATCHES: usize = 1;
pub const MAX_DEFERRED_RENDERER_MESSAGES: usize = 64;
/// Maximum validated immutable presentation retained by the browser for one revision.
pub const MAX_RENDERER_PRESENTATION_BYTES: usize = 64 * 1024 * 1024;
pub const MAX_RENDERER_DIAGNOSTIC_BYTES: usize = 16 * 1024;
/// Maximum normalized native-control text carried in one browser-to-renderer input event.
pub const MAX_RENDERER_TEXT_INPUT_BYTES: usize = 64 * 1024;
/// Maximum semantic nodes retained for one active renderer document.
pub const MAX_ACCESSIBILITY_NODES: usize = MAX_DOM_NODES;
/// A valid tree has one incoming edge per non-root node; this spare factor admits deltas while
/// preventing a compromised renderer from constructing a dense graph in the browser.
pub const MAX_ACCESSIBILITY_EDGES: usize = MAX_ACCESSIBILITY_NODES * 2;
pub const MAX_ACCESSIBILITY_NODE_TEXT_BYTES: usize = 64 * 1024;
pub const MAX_ACCESSIBILITY_TOTAL_TEXT_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_PAGE_DIAGNOSTIC_BYTES: usize = 2 * 1024 * 1024;
pub const MAX_PAGE_DIAGNOSTIC_SELECTORS: usize = 32;
pub const MAX_PAGE_DIAGNOSTIC_SELECTOR_BYTES: usize = 4 * 1024;
pub const RENDERER_MEMORY_LIMIT_BYTES: usize = 1024 * 1024 * 1024;
pub const RENDERER_STARTUP_TIMEOUT: Duration = Duration::from_secs(5);
pub const RENDERER_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(2);
pub const RENDERER_HEARTBEAT_INTERVAL: Duration = Duration::from_secs(1);
pub const RENDERER_UNRESPONSIVE_TIMEOUT: Duration = Duration::from_secs(10);
pub const RENDERER_UNRESPONSIVE_KILL_TIMEOUT: Duration = Duration::from_secs(2);
/// A first document can synchronously parse, execute blocking scripts, shape text, and lay out
/// before the renderer returns to its control pipe. The browser owns the shorter interactive
/// recovery deadline; this broker ceiling prevents the generic heartbeat from killing valid
/// first-paint work prematurely.
pub const RENDERER_FIRST_PRESENTATION_TIMEOUT: Duration = Duration::from_secs(25);

pub const MAX_SCRIPT_NAVIGATIONS: usize = 5;

pub fn bounded_utf8_prefix(input: &str, maximum: usize) -> (&str, bool) {
    if input.len() <= maximum {
        return (input, false);
    }
    let mut end = maximum;
    while !input.is_char_boundary(end) {
        end -= 1;
    }
    (&input[..end], true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn byte_prefix_never_splits_utf8() {
        let (prefix, truncated) = bounded_utf8_prefix("a🦀b", 4);
        assert_eq!(prefix, "a");
        assert!(truncated);
    }
}
