use super::*;

pub(super) fn finish_geometry_checkpoint(
    session: &RendererSession,
    document: better_web_browser::renderer_protocol::DocumentId,
) {
    // The real browser clock consumes the scroll's immediate observer checkpoint before
    // becoming idle. This fixture has no observers, so it must finish without repainting.
    session.advance_time(document, Duration::ZERO, 1).unwrap();
    for _ in 0..8 {
        match session.wait_for_event(Duration::from_secs(3)).unwrap() {
            RendererEvent::RuntimeUpdate(update) if update.document == document => {
                assert!(
                    update.runtime.errors.is_empty(),
                    "{:?}",
                    update.runtime.errors
                );
                assert!(!update.runtime.runtime_stopped);
                assert!(!update.runtime.render_requested);
                assert!(update.runtime.navigation_url.is_none());
                if update.clock_advanced {
                    if update.next_timer_micros.is_none() {
                        return;
                    }
                    assert_eq!(update.next_timer_micros, Some(0));
                    pump_ready_task(session, document, update.next_timer_micros);
                }
                assert_eq!(update.next_timer_micros, Some(0));
            }
            RendererEvent::Diagnostic { .. } => {}
            event => panic!("unexpected geometry-checkpoint event: {event:?}"),
        }
    }
    panic!("geometry and document lifecycle checkpoints did not become idle");
}

pub(super) fn wait_for_cursor(
    session: &RendererSession,
    document: better_web_browser::renderer_protocol::DocumentId,
    sequence: u64,
) -> PointerCursor {
    loop {
        match session.wait_for_event(Duration::from_secs(3)).unwrap() {
            RendererEvent::PointerCursor(result)
                if result.document == document && result.sequence == sequence =>
            {
                return result.cursor;
            }
            RendererEvent::Presentation(_)
            | RendererEvent::Diagnostic { .. }
            | RendererEvent::RuntimeUpdate(_) => {}
            event => panic!("unexpected renderer cursor event: {event:?}"),
        }
    }
}

pub(super) fn wait_for_text(
    session: &RendererSession,
    document: better_web_browser::renderer_protocol::DocumentId,
    expected: &str,
) -> better_web_browser::renderer_protocol::RendererPresentation {
    loop {
        match session.wait_for_event(Duration::from_secs(3)).unwrap() {
            RendererEvent::Presentation(presentation) if presentation.document == document => {
                if presentation_text(&presentation).contains(expected) {
                    return *presentation;
                }
            }
            RendererEvent::Diagnostic { .. } | RendererEvent::RuntimeUpdate(_) => {}
            event => panic!("unexpected renderer input event: {event:?}"),
        }
    }
}

pub(super) fn wait_for_navigation(
    session: &RendererSession,
    document: better_web_browser::renderer_protocol::DocumentId,
) -> (String, NavigationDisposition, NavigationCause) {
    loop {
        match session.wait_for_event(Duration::from_secs(3)).unwrap() {
            RendererEvent::NavigationRequested {
                document: event_document,
                url,
                disposition,
                cause,
            } if event_document == document => return (url, disposition, cause),
            RendererEvent::Presentation(_) | RendererEvent::Diagnostic { .. } => {}
            RendererEvent::RuntimeUpdate(update) if update.document == document => {
                assert_quiet_geometry_update(&update);
            }
            event => panic!("unexpected renderer navigation event: {event:?}"),
        }
    }
}

fn assert_quiet_geometry_update(
    update: &better_web_browser::renderer_protocol::RendererRuntimeUpdate,
) {
    assert!(
        update.runtime.errors.is_empty(),
        "{:?}",
        update.runtime.errors
    );
    assert!(!update.runtime.runtime_stopped);
    assert!(!update.runtime.render_requested);
    assert!(update.runtime.navigation_url.is_none());
    assert!(update.runtime.history_updates.is_empty());
    assert_eq!(update.runtime.dom_mutations, 0);
    assert!(!update.clock_advanced);
    assert_eq!(update.next_timer_micros, Some(0));
}

pub(super) fn assert_no_navigation(
    session: &RendererSession,
    document: better_web_browser::renderer_protocol::DocumentId,
    duration: Duration,
) {
    let deadline = std::time::Instant::now() + duration;
    while let Some(remaining) = deadline.checked_duration_since(std::time::Instant::now()) {
        match session.wait_for_event(remaining) {
            Ok(RendererEvent::RuntimeUpdate(update)) if update.document == document => {
                assert_quiet_geometry_update(&update);
            }
            Ok(RendererEvent::Diagnostic { .. }) => {}
            Ok(event) => panic!("secondary link activation emitted unexpected event: {event:?}"),
            Err(_) => break,
        }
    }
}

pub(super) fn presentation_text(
    presentation: &better_web_browser::renderer_protocol::RendererPresentation,
) -> String {
    presentation
        .layout
        .items
        .iter()
        .filter_map(|item| match item {
            DisplayItem::Text { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join(" ")
}
