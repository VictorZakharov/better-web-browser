//! Browser-owned admission for internal dedicated-Worker script requests.
use super::*;
use better_web_browser::renderer_protocol::FetchRequestHead;

pub(super) fn reconstruct(
    head: &FetchRequestHead,
    creator_url: &str,
) -> Result<FetchRequest, FetchError> {
    let entry = head.destination == ResourceDestination::Worker;
    if !matches!(
        head.destination,
        ResourceDestination::Worker | ResourceDestination::Script
    ) {
        return Err(invalid("Worker script has an invalid Fetch destination"));
    }
    if entry != (head.resulting_client.id != 0) {
        return Err(invalid("Worker entry must establish a response client"));
    }
    if entry
        && !FetchUrl::parse(&head.url)?
            .origin()
            .is_same_origin(&FetchUrl::parse(creator_url)?.origin())
    {
        return Err(invalid(
            "Worker entry script must be same-origin with its creator",
        ));
    }
    let expected_mode = match head.initiator {
        FetchInitiator::ClassicWorker if entry => FetchMode::SameOrigin,
        FetchInitiator::ClassicWorker => FetchMode::NoCors,
        FetchInitiator::ModuleWorker => FetchMode::Cors,
        _ => return Err(invalid("non-Worker initiator for Worker script")),
    };
    if head.mode != expected_mode {
        return Err(invalid("Worker script has an invalid request mode"));
    }
    let mut request = FetchRequest::script(&head.url, creator_url)?;
    request.context = better_web_browser::fetch::RequestContext::WorkerScript;
    Ok(request)
}

fn invalid(message: &str) -> FetchError {
    FetchError::new(FetchErrorKind::InvalidRequest, message)
}
