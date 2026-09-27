use super::*;

fn input(node: &NodeRef, code: String) -> ScriptInput {
    ScriptInput {
        source_url: "https://example.test/#canvas".into(),
        code,
        node: node.clone(),
        kind: ScriptKind::Classic,
        fetch_options: ScriptFetchOptions::for_kind(ScriptKind::Classic),
        finish_lifecycle: false,
    }
}

#[test]
fn html_canvas_bitmap_is_snapshotted_only_after_real_paint_and_resize() {
    let dom = dom::parse_with_scripting(
        r#"<canvas width=2 height=1>fallback</canvas><script>
            const canvas = document.querySelector('canvas');
            const context = canvas.getContext('2d');
            const image = context.createImageData(2, 1);
            image.data.set([200, 100, 50, 128, 0, 0, 0, 0]);
            context.putImageData(image, 0, 0);
            if (typeof __takeCanvasPresentation !== 'undefined')
                throw new Error('private paint hook escaped');
        </script>"#,
        true,
    );
    let canvas = dom.elements_named("canvas").next().unwrap();
    let script = dom.elements_named("script").next().unwrap();
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.test/");
    let outcome = runtime.execute_initial(&[input(&script, script.text_content())]);
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert!(outcome.render_requested);

    let mut snapshots = runtime.take_canvas_presentation().unwrap();
    assert_eq!(snapshots.len(), 1);
    let snapshot = snapshots.pop().unwrap();
    assert_eq!(snapshot.node, canvas.id());
    assert_eq!((snapshot.width, snapshot.height), (2, 1));
    assert_eq!(snapshot.pixels.unwrap(), [200, 100, 50, 128, 0, 0, 0, 0]);
    assert!(runtime.take_canvas_presentation().unwrap().is_empty());

    let outcome = runtime.execute_additional_with_loader(
        &[input(
            &script,
            "setTimeout(() => context.fillRect(0, 0, 1, 1), 10000)".into(),
        )],
        None,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    let timer = runtime.advance_time(std::time::Duration::from_secs(11), 1);
    assert!(timer.errors.is_empty(), "{:?}", timer.errors);
    assert!(timer.render_requested);
    assert_eq!(runtime.take_canvas_presentation().unwrap().len(), 1);

    let outcome = runtime.execute_additional_with_loader(
        &[input(&script, "canvas.setAttribute('width', '1')".into())],
        None,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    let mut snapshots = runtime.take_canvas_presentation().unwrap();
    let snapshot = snapshots.pop().unwrap();
    assert_eq!((snapshot.width, snapshot.height), (1, 1));
    assert_eq!(snapshot.pixels.unwrap(), [0, 0, 0, 0]);
}

#[test]
fn same_realm_offscreen_placeholder_presents_its_real_bitmap() {
    let dom = dom::parse_with_scripting(
        r#"<canvas width=1 height=1></canvas><script>
            const placeholder = document.querySelector('canvas');
            const surface = placeholder.transferControlToOffscreen();
            const pixels = surface.getContext('2d').createImageData(1, 1);
            pixels.data.set([3, 7, 11, 255]);
            surface.getContext('2d').putImageData(pixels, 0, 0);
        </script>"#,
        true,
    );
    let script = dom.elements_named("script").next().unwrap();
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.test/");
    let outcome = runtime.execute_initial(&[input(&script, script.text_content())]);
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    let snapshots = runtime.take_canvas_presentation().unwrap();
    assert_eq!(snapshots.len(), 1);
    assert_eq!(
        snapshots[0].pixels.as_deref(),
        Some([3, 7, 11, 255].as_slice())
    );
    let outcome =
        runtime.execute_additional_with_loader(&[input(&script, "surface.width = 1".into())], None);
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    let snapshots = runtime.take_canvas_presentation().unwrap();
    assert_eq!(snapshots.len(), 1);
    assert_eq!(
        snapshots[0].pixels.as_deref(),
        Some([0, 0, 0, 0].as_slice())
    );
}

#[test]
fn canvas_export_retains_overflowed_dirty_bitmap_and_wakes_the_next_checkpoint() {
    const {
        assert!(
            crate::limits::MAX_CANVAS_PIXELS * 4 <= crate::limits::MAX_PAGE_DECODED_IMAGE_BYTES
        );
    }
    let html = format!(
        "{}<script>for (const canvas of document.querySelectorAll('canvas')) \
         canvas.getContext('2d').fillRect(0, 0, 1, 1);</script>",
        "<canvas width=2048 height=2048></canvas>".repeat(5)
    );
    let dom = dom::parse_with_scripting(&html, true);
    let never_exported = dom.elements_named("canvas").nth(4).unwrap().id();
    let script = dom.elements_named("script").next().unwrap();
    let mut runtime = ScriptRuntime::new(dom.document.clone(), "https://example.test/");
    let outcome = runtime.execute_initial(&[input(&script, script.text_content())]);
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);

    let first = runtime.take_canvas_presentation().unwrap();
    assert_eq!(first.len(), 4);
    assert!(first.iter().all(|snapshot| {
        snapshot
            .pixels
            .as_ref()
            .is_some_and(|pixels| pixels.len() == 2048 * 2048 * 4)
    }));
    drop(first);
    assert_eq!(runtime.next_timer_delay(), Some(std::time::Duration::ZERO));
    let wake = runtime.advance_time(std::time::Duration::ZERO, 1);
    assert!(wake.errors.is_empty(), "{:?}", wake.errors);
    assert!(wake.render_requested);

    let repaint = runtime.execute_additional_with_loader(
        &[input(
            &script,
            "for (const canvas of [...document.querySelectorAll('canvas')].slice(0, 4)) \
             canvas.getContext('2d').fillRect(0, 0, 1, 1)"
                .into(),
        )],
        None,
    );
    assert!(repaint.errors.is_empty(), "{:?}", repaint.errors);
    let second = runtime.take_canvas_presentation().unwrap();
    assert_eq!(second.len(), 4);
    assert_eq!(second[0].node, never_exported);
    assert_eq!(second[0].pixels.as_ref().unwrap().len(), 2048 * 2048 * 4);
    assert_eq!(runtime.next_timer_delay(), Some(std::time::Duration::ZERO));
    let final_wake = runtime.advance_time(std::time::Duration::ZERO, 1);
    assert!(final_wake.render_requested);
    assert_eq!(runtime.take_canvas_presentation().unwrap().len(), 1);
    assert!(runtime.take_canvas_presentation().unwrap().is_empty());
    assert_eq!(runtime.next_timer_delay(), None);
}
