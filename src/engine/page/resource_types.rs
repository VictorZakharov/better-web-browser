//! Element-owned requests and prepared scripts retained by the document loader.

use crate::engine::dom::{NodeId, NodeRef};
use crate::engine::script::{ScriptFetchOptions, ScriptKind};
use crate::fetch::{CredentialsMode, ReferrerPolicy, RequestMode};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PageResource {
    /// A speculative origin resolution. `preconnect` currently performs the DNS portion of
    /// its handshake; no HTTP request or response is associated with either hint.
    OriginHint {
        origin: String,
    },
    /// Best-effort fetch of a same-origin document for a future navigation. The browser owns
    /// admission and cache reuse; speculative bytes are never installed in this document.
    Prefetch {
        url: String,
    },
    /// A link preload fetches into a document-scoped cache; it does not execute or apply bytes.
    Preload {
        url: String,
        as_type: PreloadAs,
        mode: RequestMode,
        credentials: CredentialsMode,
        referrer_policy: ReferrerPolicy,
        integrity: String,
        /// Link nonce is part of script CSP admission and cannot be dropped on cache reuse.
        nonce: Option<String>,
    },
    Stylesheet {
        url: String,
    },
    Image {
        url: String,
    },
    Media {
        url: String,
        node: NodeId,
        kind: MediaElementKind,
        /// The selected direct child, or None when the media element's src wins.
        source_node: Option<NodeId>,
        /// Distinguishes replacement/fallback fetches even when the URL repeats.
        selection_id: u64,
        mode: RequestMode,
        credentials: CredentialsMode,
    },
    Script {
        url: String,
        kind: ScriptKind,
        fetch_options: ScriptFetchOptions,
        script_source: crate::fetch::csp::ScriptSource,
    },
    Font {
        url: String,
        family: String,
        weight: u16,
        italic: bool,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MediaElementKind {
    Audio,
    Video,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PreloadAs {
    Script,
    ModuleScript,
    Style,
    Image,
    Font,
}

#[derive(Debug, Clone)]
pub struct PageScript {
    pub node: NodeRef,
    pub source_url: String,
    /// Frozen when HTML prepares this element; later mutation/removal cannot weaken SRI.
    pub integrity: String,
    pub code: Option<String>,
    pub kind: ScriptKind,
    pub fetch_options: ScriptFetchOptions,
    pub blocks_first_paint: bool,
    pub executes_after_parsing: bool,
}
