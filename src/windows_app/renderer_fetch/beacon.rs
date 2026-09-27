//! One-way Beacon delivery, owned by the browser rather than the retiring document.

use super::*;
use better_web_browser::fetch::is_cors_safelisted_request_header;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicUsize, Ordering};

const MAX_IN_FLIGHT_BEACONS: usize = 32;
static IN_FLIGHT_BEACONS: OnceLock<Arc<AtomicUsize>> = OnceLock::new();

struct InFlightPermit(Arc<AtomicUsize>);

impl Drop for InFlightPermit {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}

fn reserve(counter: Arc<AtomicUsize>, limit: usize) -> Option<InFlightPermit> {
    counter
        .fetch_update(Ordering::AcqRel, Ordering::Acquire, |current| {
            (current < limit).then_some(current + 1)
        })
        .ok()
        .map(|_| InFlightPermit(counter))
}

pub(super) fn submit(
    renderer: RendererFetchRequest,
    document: DocumentId,
    document_url: &str,
    client: &Arc<winhttp::HttpClient>,
    registry: &RendererFetchRegistry,
) -> Result<(), String> {
    renderer.validate().map_err(|error| error.to_string())?;
    validate_document_identity(document, renderer.head.document)
        .map_err(|error| error.to_string())?;
    let head = &renderer.head;
    let body_bytes = renderer.body.len();
    // The untrusted renderer cannot turn a Beacon into a credentialed arbitrary method,
    // response read, child navigation, or a request with author-controlled headers.
    if head.destination != ResourceDestination::Fetch
        || head.method != "POST"
        || head.resulting_client.id != 0
        || head.embedding_client.id != 0
        || head.script_source.is_some()
        || !matches!(head.mode, FetchMode::Cors | FetchMode::NoCors)
        || head.credentials != FetchCredentials::Include
        || head.cache != FetchCache::Default
        || head.redirect != FetchRedirect::Follow
        || !matches!(head.referrer, FetchReferrer::Client | FetchReferrer::Url(_))
        || head.referrer_policy != FetchReferrerPolicy::StrictOriginWhenCrossOrigin
        || head.headers.len() > 1
        || head
            .headers
            .iter()
            .any(|(name, _)| !name.eq_ignore_ascii_case("content-type"))
        || body_bytes > better_web_browser::limits::MAX_KEEPALIVE_BODY_BYTES
    {
        return Err("invalid Beacon intent".into());
    }
    let owner = registry
        .clients
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .resolve(document, document_url, head.client)
        .map_err(|error| error.to_string())?;
    let mut request = reconstruct(&owner.url, renderer).map_err(|error| error.to_string())?;
    validate_beacon_request(&request)?;
    request.origin = Some(owner.origin);
    request.policy = owner.policy;
    // A Beacon has no response callback; cap and discard its response without involving
    // the renderer's response stream (which is retired on navigation).
    request.response_body_limit = 64 * 1024;
    let counter = Arc::clone(IN_FLIGHT_BEACONS.get_or_init(|| Arc::new(AtomicUsize::new(0))));
    let Some(permit) = reserve(counter, MAX_IN_FLIGHT_BEACONS) else {
        // Admission was already acknowledged in the renderer. A full browser worker pool
        // is an asynchronous delivery failure, not a fatal page-engine error.
        return Ok(());
    };
    let Some(upload_permit) = registry.keepalive_budget.reserve(document, body_bytes) else {
        // A queued Beacon is best-effort; it shares the document's Fetch keepalive
        // upload budget and never bypasses a concurrent fetch(..., {keepalive:true}).
        return Ok(());
    };
    let client = Arc::clone(client);
    std::thread::Builder::new()
        .name("breeze-beacon".into())
        .spawn(move || {
            let _permit = permit;
            let _upload_permit = upload_permit;
            if let Ok(mut response) = client.fetch_stream(request) {
                while matches!(response.next_chunk(), Ok(Some(_))) {}
            }
        })
        .map(|_| ())
        .map_err(|error| format!("start Beacon delivery: {error}"))
}

fn validate_beacon_request(request: &FetchRequest) -> Result<(), String> {
    if request.url.parsed().is_none()
        || (request.mode == RequestMode::NoCors
            && request
                .headers
                .iter()
                .any(|header| !is_cors_safelisted_request_header(header)))
    {
        return Err("invalid Beacon URL or no-CORS Content-Type".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn worker_admission_is_bounded_and_released_after_completion() {
        let counter = Arc::new(AtomicUsize::new(0));
        let first = reserve(Arc::clone(&counter), 2).unwrap();
        let second = reserve(Arc::clone(&counter), 2).unwrap();
        assert!(reserve(Arc::clone(&counter), 2).is_none());
        drop(first);
        let third = reserve(Arc::clone(&counter), 2).unwrap();
        assert_eq!(counter.load(Ordering::Acquire), 2);
        drop((second, third));
        assert_eq!(counter.load(Ordering::Acquire), 0);
    }

    #[test]
    fn forged_no_cors_json_beacon_is_rejected_before_network() {
        let document = DocumentId::new(1).unwrap();
        let mut wire = super::super::tests::intent(document, "https://api.example.test/collect");
        wire.head.initiator = FetchInitiator::Beacon;
        wire.head.method = "POST".into();
        wire.head.mode = FetchMode::NoCors;
        wire.head.credentials = FetchCredentials::Include;
        wire.head.headers = vec![("content-type".into(), "application/json".into())];
        let request = reconstruct("https://example.test/", wire).unwrap();
        assert!(validate_beacon_request(&request).is_err());
    }

    #[test]
    fn no_cors_text_beacon_remains_admissible() {
        let document = DocumentId::new(1).unwrap();
        let mut wire = super::super::tests::intent(document, "https://api.example.test/collect");
        wire.head.initiator = FetchInitiator::Beacon;
        wire.head.method = "POST".into();
        wire.head.mode = FetchMode::NoCors;
        wire.head.credentials = FetchCredentials::Include;
        wire.head.headers = vec![("content-type".into(), "text/plain;charset=UTF-8".into())];
        let request = reconstruct("https://example.test/", wire).unwrap();
        assert!(validate_beacon_request(&request).is_ok());
    }

    #[test]
    fn data_url_beacon_is_rejected_before_network() {
        let document = DocumentId::new(1).unwrap();
        let mut wire = super::super::tests::intent(document, "data:text/plain,leak");
        wire.head.initiator = FetchInitiator::Beacon;
        wire.head.method = "POST".into();
        wire.head.mode = FetchMode::NoCors;
        wire.head.credentials = FetchCredentials::Include;
        let request = reconstruct("https://example.test/", wire).unwrap();
        assert!(validate_beacon_request(&request).is_err());
    }
}
