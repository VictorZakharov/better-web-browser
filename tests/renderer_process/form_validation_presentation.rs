use super::support::*;
use better_web_browser::engine::{ControlKind, DisplayItem};
use better_web_browser::renderer_process::RendererSession;
use better_web_browser::renderer_protocol::{DocumentNodeId, RendererPresentation};
use std::time::{Duration, Instant};

const DENSE_FIXTURE: &str = include_str!("../fixtures/form-validation-dense.html");

/// Native invalid styling repaints through the style/paint path, with and
/// without script on the page.
#[test]
fn validation_style_repaints_natively() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    for script in ["", "<script>document.body.dataset.ready='yes'</script>"] {
        let session = RendererSession::launch(options()).expect("hidden renderer");
        let initial = load_html_document(
            &session,
            125,
            &format!(
                r#"<!doctype html><style>body{{margin:0}}
                input{{display:block;width:200px;height:30px}}
                span{{display:block;width:100px;height:30px;background:rgb(0,128,0)}}
                input:invalid+span{{background:rgb(255,0,0)}}</style>
                <input required><span>flag</span>{script}"#
            ),
        );
        acknowledge(&session, &initial);
        let red = |presentation: &RendererPresentation| {
            presentation.layout.items.iter().any(|item| match item {
                DisplayItem::SolidRect { rect, color, .. } => {
                    rect.width == 100.0 && color.red == 255
                }
                _ => false,
            })
        };
        let green = |presentation: &RendererPresentation| {
            presentation.layout.items.iter().any(|item| match item {
                DisplayItem::SolidRect { rect, color, .. } => {
                    rect.width == 100.0 && color.green == 128
                }
                _ => false,
            })
        };
        assert!(red(&initial), "invalid flag paints, script={script:?}");
        let target = initial
            .layout
            .items
            .iter()
            .find_map(|item| match item {
                DisplayItem::Control(control) if control.kind == ControlKind::Text => {
                    DocumentNodeId::new(control.node_id.to_wire()).ok()
                }
                _ => None,
            })
            .expect("input control target");
        send_text(&session, initial.document, 1, target, "ok");
        let painted = wait_for_presentation(&session, initial.document, "valid repaint", |p| {
            green(p) && !red(p)
        });
        assert!(green(&painted) && !red(&painted), "script={script:?}");
    }
}

/// A 400-control form still presents a native edit promptly (cascade
/// proportionality smoke cover for the dense fixture).
#[test]
fn dense_form_native_edit_presents() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let session = RendererSession::launch(options()).expect("hidden renderer");
    let initial = load_html_document(&session, 126, DENSE_FIXTURE);
    acknowledge(&session, &initial);
    let controls = initial
        .layout
        .items
        .iter()
        .filter(|item| {
            matches!(item, DisplayItem::Control(control) if control.kind == ControlKind::Text)
        })
        .count();
    assert!(controls >= 400, "dense fixture controls: {controls}");
    let first = initial
        .layout
        .items
        .iter()
        .find_map(|item| match item {
            DisplayItem::Control(control) if control.kind == ControlKind::Text => {
                DocumentNodeId::new(control.node_id.to_wire()).ok()
            }
            _ => None,
        })
        .expect("dense input target");
    let started = Instant::now();
    send_text(&session, initial.document, 1, first, "edited!");
    let updated = wait_for_presentation(&session, initial.document, "dense edit", |p| {
        p.layout.items.iter().any(|item| {
            matches!(item, DisplayItem::Control(control)
                if control.kind == ControlKind::Text && control.value == "edited!")
        })
    });
    let elapsed = started.elapsed();
    assert!(
        elapsed < Duration::from_secs(10),
        "dense native edit presented in {elapsed:?}"
    );
    assert!(
        updated
            .layout
            .items
            .iter()
            .filter(|item| matches!(
                item,
                DisplayItem::Control(control) if control.kind == ControlKind::Text
            ))
            .count()
            >= 400
    );
}
