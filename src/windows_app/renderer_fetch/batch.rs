//! Per-document admission and bounded scheduling of renderer Fetch intents.

use super::*;
use better_web_browser::limits::MAX_PARALLEL_RENDERER_FETCHES;
use std::time::Instant;

pub(in crate::windows_app) fn spawn_fetch_batch(batch: RendererFetchBatch) -> Result<(), String> {
    let RendererFetchBatch {
        tab_id,
        document,
        document_url,
        requests,
        client,
        signal,
        registry,
        sink,
        tab_router,
    } = batch;
    registry
        .clients
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .activate(document);
    let mut jobs = Vec::new();
    for request in requests {
        if request.head.initiator == FetchInitiator::Beacon {
            // Beacon is one-way and best-effort. A rejected intent or a failed
            // delivery worker cannot be reported to its already-returned JS
            // caller, and must not terminate the page engine.
            let _ = beacon::submit(request, document, &document_url, &client, &registry);
        } else if request.head.keepalive {
            let request_id = request.head.request_id;
            match keepalive::prepare(request, document, &document_url, &registry) {
                Ok(prepared) => {
                    // Only an explicit AbortSignal can cancel this upload; document
                    // retirement must not revoke a request already admitted here.
                    let request_signal = registry.register(document, request_id);
                    jobs.push(pump::Job::Keepalive(Box::new(prepared), request_signal));
                }
                Err(error) => {
                    let _ = send_failure(&sink, request_id, &error);
                }
            }
        } else {
            let request_id = request.head.request_id;
            let request_signal = registry.register(document, request_id);
            jobs.push(pump::Job::Request(
                Box::new(request),
                signal.any(&request_signal),
            ));
        }
    }
    if jobs.is_empty() {
        return Ok(());
    }
    std::thread::Builder::new()
        .name(format!("breeze-renderer-fetch-{}", tab_id.get()))
        .spawn(move || {
            let started = Instant::now();
            // Keep every available network slot useful. Partitioning requests into fixed waves
            // lets one slow media or font response prevent later styles and images from starting.
            let bytes = scheduler::execute_bounded(jobs, MAX_PARALLEL_RENDERER_FETCHES, |job| {
                let request_id = job.id();
                let started = job.started();
                let step = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    job.step(&client, &sink, document, &document_url, &registry)
                }))
                .unwrap_or_else(|_| {
                    let error =
                        FetchError::new(FetchErrorKind::Network, "browser Fetch worker panicked");
                    if started {
                        let _ = sink.abort(request_id, wire_error(&error));
                    } else {
                        let _ = send_failure(&sink, request_id, &error);
                    }
                    scheduler::Step::Done(0)
                });
                if matches!(step, scheduler::Step::Done(_)) {
                    registry.complete(document, request_id);
                }
                step
            });
            let completion = Box::new(RendererFetchCompletion {
                document,
                bytes,
                network_time: started.elapsed(),
            });
            let pointer = Box::into_raw(completion);
            let posted = tab_router.destination(tab_id).is_some_and(|window| unsafe {
                PostMessageW(
                    window as Hwnd,
                    WM_APP_RENDERER_FETCH_COMPLETE,
                    tab_id.get() as usize,
                    pointer as isize,
                ) != 0
            });
            if !posted {
                unsafe { drop(Box::from_raw(pointer)) };
            }
        })
        .map(|_| ())
        .map_err(|error| format!("start renderer Fetch worker: {error}"))
}
