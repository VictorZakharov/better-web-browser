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

pub const MAX_CSS_SOURCE_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_CSS_RULES: usize = 20_000;
pub const MAX_CSS_NESTING_DEPTH: usize = 64;
pub const MAX_CSS_DECLARATIONS_PER_RULE: usize = 256;
pub const MAX_ADOPTED_STYLESHEETS: usize = 256;
pub const MAX_ADOPTED_STYLESHEET_PAYLOAD_BYTES: usize = 8 * 1024 * 1024;

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

pub const MAX_SCRIPT_BYTES: usize = 16 * 1024 * 1024;
/// Aggregate source admitted by one window realm. Individual scripts retain the smaller boundary
/// above while component-heavy applications can load multiple independently bounded bundles.
pub const MAX_PAGE_SCRIPT_BYTES: usize = 32 * 1024 * 1024;
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
/// Aggregate decoded image identities retained by a document and accepted in one presentation.
/// HTML images, CSS images, inline SVGs, the video placeholder, and bounded media sessions are
/// admitted independently, so the presentation protocol must cover their combined maximum.
pub const MAX_PRESENTED_IMAGES: usize =
    MAX_PAGE_IMAGES + MAX_STYLE_IMAGES + MAX_INLINE_SVGS + MAX_MEDIA_SESSIONS_PER_TAB + 1;
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
/// Script API responses are delivered incrementally, so this is a total-transfer safety quota,
/// not a resident-memory allocation. Buffered resources keep the smaller response-body limit.
pub const MAX_RENDERER_FETCH_STREAM_BYTES: usize = 64 * 1024 * 1024;
pub const MAX_RENDERER_FETCH_REQUESTS_PER_BATCH: usize = 256;
pub const MAX_PARALLEL_RENDERER_FETCHES: usize = 8;
pub const MAX_RENDERER_FETCH_BATCH_BODY_BYTES: usize = 32 * 1024 * 1024;
pub const MAX_RENDERER_FETCH_BATCH_METADATA_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_RENDERER_FETCH_HEADERS: usize = 256;
pub const MAX_FETCH_HEADER_NAME_BYTES: usize = 1024;
pub const MAX_FETCH_HEADER_VALUE_BYTES: usize = 16 * 1024;
pub const MAX_QUEUED_BROWSER_COMMANDS: usize = 8;
/// Upper bound for already-validated messages waiting on a renderer command pipe. Document,
/// storage, and Fetch byte limits bound the payload memory represented by these messages; this
/// count additionally prevents an unresponsive page from accumulating small input indefinitely.
pub const MAX_QUEUED_BROWSER_WRITES: usize = 4_096;
/// Valid native input retained per tab while the renderer command pipe applies backpressure.
/// This is deliberately no larger than the broker command channel so a stalled renderer cannot
/// make browser-owned memory grow with the rate of Windows input messages.
pub const MAX_PENDING_RENDERER_INPUTS: usize = MAX_QUEUED_BROWSER_COMMANDS;
pub const MAX_QUEUED_RENDERER_IPC_MESSAGES: usize = 8;
pub const MAX_QUEUED_RENDERER_EVENTS: usize = 256;
pub const MAX_QUEUED_RENDERER_FETCH_BATCHES: usize = 1;
/// Ordinary page commands retained while a synchronous renderer fetch waits for the browser.
pub const MAX_DEFERRED_RENDERER_MESSAGES: usize = 64;
/// Reserved atomic transfer capacity for the one browser-authoritative state snapshot allowed in
/// flight. Its payload remains bounded independently by the per-origin entry and byte quotas.
pub const MAX_DEFERRED_RENDERER_STATE_MESSAGES: usize = MAX_STORAGE_ENTRIES_PER_ORIGIN + 2;
/// Maximum validated immutable presentation retained by the browser for one revision.
pub const MAX_RENDERER_PRESENTATION_BYTES: usize = 64 * 1024 * 1024;
pub const MAX_RENDERER_DIAGNOSTIC_BYTES: usize = 16 * 1024;
pub const MAX_RUNTIME_REPORT_ENTRIES: usize = 512;
pub const MAX_RUNTIME_REPORT_TEXT_BYTES: usize = 64 * 1024;
/// Maximum normalized native-control text carried in one browser-to-renderer input event.
pub const MAX_RENDERER_TEXT_INPUT_BYTES: usize = 64 * 1024;
/// Maximum semantic nodes retained for one active renderer document.
pub const MAX_ACCESSIBILITY_NODES: usize = MAX_DOM_NODES;
/// A valid tree has one incoming edge per non-root node; this spare factor admits deltas while
/// preventing a compromised renderer from constructing a dense graph in the browser.
pub const MAX_ACCESSIBILITY_EDGES: usize = MAX_ACCESSIBILITY_NODES * 2;
pub const MAX_ACCESSIBILITY_NODE_TEXT_BYTES: usize = 64 * 1024;
pub const MAX_ACCESSIBILITY_TOTAL_TEXT_BYTES: usize = 4 * 1024 * 1024;
/// Finite document-space extent accepted for renderer presentation geometry. Layout can produce
/// non-finite intermediate values for unsupported or cyclic CSS; the renderer sanitizes those
/// before IPC while the browser decoder retains this fail-closed boundary for untrusted payloads.
pub const MAX_PRESENTATION_COORDINATE: f32 = 10_000_000.0;
/// Finite document-space extent accepted for native accessibility geometry. Page layout may
/// calculate larger or non-finite intermediate rectangles; the renderer sanitizes those before
/// crossing IPC while the decoder retains this fail-closed boundary for untrusted payloads.
pub const MAX_ACCESSIBILITY_COORDINATE: f32 = 10_000_000.0;
pub const MAX_PAGE_DIAGNOSTIC_BYTES: usize = 2 * 1024 * 1024;
pub const MAX_PAGE_DIAGNOSTIC_SELECTORS: usize = 32;
pub const MAX_PAGE_DIAGNOSTIC_SELECTOR_BYTES: usize = 4 * 1024;
pub const RENDERER_MEMORY_LIMIT_BYTES: usize = 1024 * 1024 * 1024;
pub const RENDERER_STARTUP_TIMEOUT: Duration = Duration::from_secs(5);
pub const RENDERER_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(2);
pub const RENDERER_HEARTBEAT_INTERVAL: Duration = Duration::from_secs(1);
// First presentation has its own larger allowance below. Once a page is interactive, a command
// task that cannot answer the control-plane heartbeat is reported quickly. The in-process V8
// watchdog has a shorter per-execution deadline; this outer boundary still contains native work
// and failures outside JavaScript while giving the isolated renderer a bounded recovery window.
pub const RENDERER_UNRESPONSIVE_TIMEOUT: Duration = Duration::from_secs(3);
pub const RENDERER_UNRESPONSIVE_KILL_TIMEOUT: Duration = Duration::from_secs(12);
/// A first document can synchronously parse, execute blocking scripts, shape text, and lay out
/// before the renderer returns to its control pipe. The browser owns the shorter interactive
/// recovery deadline; this broker ceiling prevents the generic heartbeat from killing valid
/// first-paint work prematurely.
pub const RENDERER_FIRST_PRESENTATION_TIMEOUT: Duration = Duration::from_secs(25);

// Media-process budgets are deliberately independent from page-renderer limits. A later decode
// slice may reduce these further after measuring real H.264/AAC queues; raising one requires a
// hostile-media review and an update to ADR 0007.
pub const MAX_MEDIA_CONTROL_PAYLOAD: usize = 4 * 1024;
pub const MAX_MEDIA_DATA_CHUNK_BYTES: usize = 256 * 1024;
pub const MAX_MEDIA_SESSIONS_PER_TAB: usize = 4;
/// The first playback slice owns one decoder and one document clock. Raise this only when the
/// renderer and worker both retain independent state for every admitted media element.
pub const MAX_ACTIVE_MEDIA_ELEMENTS_PER_DOCUMENT: usize = 1;
pub const MAX_MEDIA_TRACKS: usize = 8;
pub const MAX_MEDIA_DIMENSION: u32 = 8_192;
pub const MAX_MEDIA_ENCODED_BYTES: usize = 64 * 1024 * 1024;
pub const MAX_MEDIA_DECODED_FRAME_BYTES: usize = 128 * 1024 * 1024;
pub const MAX_MEDIA_DECODED_AUDIO_SAMPLE_BYTES: usize = 1024 * 1024;
pub const MAX_MEDIA_DECODED_AUDIO_QUEUE_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_MEDIA_ENCODED_QUEUE_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_MEDIA_DECODED_FRAMES: usize = 4;
/// Decoded samples are consumed immediately, but cumulative counters are still bounded so hostile
/// inputs cannot keep a worker decoding an effectively infinite timeline.
pub const MAX_MEDIA_DECODED_SAMPLES: usize = 16_384;
pub const MAX_MEDIA_DECODED_SOURCE_BYTES: u64 = 512 * 1024 * 1024;
pub const MAX_MEDIA_DURATION_100NS: u64 = 60 * 60 * 10_000_000;
pub const MAX_MEDIA_DECODER_CANDIDATES: usize = 64;
pub const MEDIA_PROCESS_MEMORY_LIMIT_BYTES: usize = 512 * 1024 * 1024;
pub const MEDIA_STARTUP_TIMEOUT: Duration = Duration::from_secs(5);
pub const MEDIA_COMMAND_TIMEOUT: Duration = Duration::from_secs(5);
pub const MEDIA_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(2);

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

    #[test]
    fn page_script_budget_preserves_source_and_resource_boundaries() {
        const {
            assert!(MAX_SCRIPT_BYTES < MAX_PAGE_SCRIPT_BYTES);
            assert!(MAX_PAGE_SCRIPT_BYTES <= PAGE_RESOURCE_BUDGET as usize);
            assert!(MAX_CSS_SOURCE_BYTES <= PAGE_RESOURCE_BUDGET as usize);
            assert!(MAX_SCRIPT_BYTES <= MAX_RESPONSE_BODY_BYTES);
        }
    }

    #[test]
    fn presented_image_limit_covers_every_decoded_image_category() {
        const {
            assert!(MAX_PRESENTED_IMAGES > MAX_PAGE_IMAGES);
            assert!(MAX_PRESENTED_IMAGES <= u32::MAX as usize);
        }
    }
}
