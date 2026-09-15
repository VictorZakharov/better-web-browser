//! Typed streaming deliveries to the owning script realm.
use super::*;
#[derive(Debug)]
pub(in crate::renderer_process::child) enum ScriptFetchDelivery {
    Head {
        document: DocumentId,
        head: FetchResponseHead,
    },
    Chunk {
        document: DocumentId,
        request_id: u64,
        bytes: Vec<u8>,
    },
    End {
        document: DocumentId,
        request_id: u64,
    },
    Abort {
        document: DocumentId,
        request_id: u64,
        error: crate::renderer_protocol::BrowserFetchError,
    },
}

impl ScriptFetchDelivery {
    pub(in crate::renderer_process::child) fn document(&self) -> DocumentId {
        match self {
            Self::Head { document, .. }
            | Self::Chunk { document, .. }
            | Self::End { document, .. }
            | Self::Abort { document, .. } => *document,
        }
    }
}
