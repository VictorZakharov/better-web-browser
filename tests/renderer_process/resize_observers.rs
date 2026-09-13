//! ResizeObserver settles in the isolated renderer before paint; IO remains a queued task.
use super::*;
use better_web_browser::renderer_protocol::{DocumentId, PresentationAcknowledgement};

fn next_console(session: &RendererSession, document: DocumentId) -> Vec<String> {
    loop {
        match session.wait_for_event(Duration::from_secs(3)).unwrap() {
            RendererEvent::Presentation(p) if p.document == document => {
                assert!(p.runtime.errors.is_empty(), "{:?}", p.runtime.errors);
                session
                    .acknowledge_presentation(PresentationAcknowledgement {
                        document,
                        revision: p.revision,
                        presented: true,
                        controls_applied: true,
                    })
                    .unwrap();
                return p.runtime.console;
            }
            RendererEvent::RuntimeUpdate(u) if u.document == document => {
                assert!(u.runtime.errors.is_empty(), "{:?}", u.runtime.errors);
                return u.runtime.console;
            }
            RendererEvent::Diagnostic { .. } => {}
            event => panic!("unexpected observer event: {event:?}"),
        }
    }
}

#[test]
fn resize_observer_mutations_are_visible_in_first_presentation() {
    let _serial = SERIAL.lock().unwrap_or_else(|p| p.into_inner());
    let mut session = RendererSession::launch(options()).unwrap();
    let initial = load_html_document(
        &session,
        901,
        r#"<!doctype html>
        <style>#target{width:120px;height:80px;padding:10px;border:2px solid}</style>
        <div id=target><p id=label>unsettled</p></div><script>
          new ResizeObserver(entries => {
            const e = entries[0];
            document.getElementById('label').textContent = 'settled ' + e.contentRect.width;
            document.title = [e.contentRect.x,e.contentRect.y,e.contentRect.width,
                e.contentRect.height,e.borderBoxSize[0].inlineSize].join(':');
          }).observe(document.getElementById('target'));
        </script>"#,
    );
    assert!(
        initial.runtime.errors.is_empty(),
        "{:?}",
        initial.runtime.errors
    );
    assert_eq!(initial.title, "10:10:120:80:144");
    let text = initial
        .layout
        .items
        .iter()
        .filter_map(|item| match item {
            DisplayItem::Text { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join(" ");
    assert!(
        text.split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .contains("settled 120"),
        "{text}"
    );
    assert!(!text.contains("unsettled"), "no stale pre-observer paint");
    session.shutdown().unwrap();
}

#[test]
fn resize_observer_loop_errors_defer_self_resize_to_another_checkpoint() {
    let _serial = SERIAL.lock().unwrap_or_else(|p| p.into_inner());
    let mut session = RendererSession::launch(options()).unwrap();
    let initial = load_html_document(
        &session,
        902,
        r#"<!doctype html>
        <div id=target style="width:100px;height:30px"></div><script>
          const target = document.getElementById('target');
          let calls = 0;
          addEventListener('error', event => {
            if (event.message === 'ResizeObserver loop completed with undelivered notifications.') {
              console.log('loop error'); event.preventDefault();
            }
          });
          new ResizeObserver(entries => {
            console.log('resize:' + ++calls);
            target.style.width = entries[0].contentRect.width + 1 + 'px';
          }).observe(target);
        </script>"#,
    );
    assert_eq!(
        initial.runtime.console,
        ["log: resize:1", "log: loop error"]
    );
    session
        .advance_time(initial.document, Duration::from_millis(16), 4)
        .unwrap();
    assert_eq!(
        next_console(&session, initial.document),
        ["log: resize:2", "log: loop error"]
    );
    session.shutdown().unwrap();
}

#[test]
fn resize_observer_registration_without_mutation_wakes_renderer() {
    let _serial = SERIAL.lock().unwrap_or_else(|p| p.into_inner());
    let mut session = RendererSession::launch(options()).unwrap();
    let initial = load_html_document(
        &session,
        903,
        r#"<!doctype html>
        <div id=target style="width:100px;height:30px"></div><script>
          setTimeout(() => {
            new ResizeObserver((entries, observer) => {
              console.log('late:' + entries[0].contentRect.width); observer.disconnect();
            }).observe(document.getElementById('target'));
          }, 20);
        </script>"#,
    );
    let mut logs = initial.runtime.console;
    for _ in 0..4 {
        session
            .advance_time(initial.document, Duration::from_millis(20), 4)
            .unwrap();
        logs.extend(next_console(&session, initial.document));
    }
    assert_eq!(logs, ["log: late:100"]);
    session.shutdown().unwrap();
}

#[test]
fn resize_observer_sees_animation_frame_mutations_before_its_paint() {
    let _serial = SERIAL.lock().unwrap_or_else(|p| p.into_inner());
    let mut session = RendererSession::launch(options()).unwrap();
    let initial = load_html_document(
        &session,
        904,
        r#"<!doctype html>
        <div id=target style="width:100px;height:30px"></div><script>
          const target = document.getElementById('target');
          new ResizeObserver(entries => console.log('resize:' + entries[0].contentRect.width)).observe(target);
          requestAnimationFrame(() => { console.log('frame'); target.style.width = '200px'; });
        </script>"#,
    );
    assert_eq!(initial.runtime.console, ["log: resize:100"]);
    let mut logs = Vec::new();
    for _ in 0..4 {
        session
            .advance_time(initial.document, Duration::from_millis(16), 4)
            .unwrap();
        logs.extend(next_console(&session, initial.document));
    }
    assert_eq!(logs, ["log: frame", "log: resize:200"]);
    session.shutdown().unwrap();
}
