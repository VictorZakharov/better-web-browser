use super::*;

#[test]
fn child_image_update_survives_discarded_paint_snapshot_until_emitted() {
    let (_dom, mut runtime) = start(
        r#"<body><script>
            const frame = document.createElement('iframe');
            frame.srcdoc = '<canvas width=2 height=1></canvas><script>' +
                'const context = document.querySelector("canvas").getContext("2d");' +
                'context.fillStyle = "blue"; context.fillRect(1,0,1,1);' +
                '<\/script>';
            document.body.append(frame);
        </script>"#,
    );
    runtime.finish_document_lifecycle();
    drain(&mut runtime);
    let snapshots = runtime.frame_paint_snapshots();
    let frame = snapshots.first().expect("srcdoc frame snapshot");
    let key = frame
        .images
        .keys()
        .find(|key| key.starts_with("breeze-internal:canvas:"))
        .expect("child Canvas bitmap")
        .clone();
    assert!(frame.image_updates.contains(&key));
    assert_eq!(&*frame.images[&key].bgra, &[0, 0, 0, 0, 255, 0, 0, 255]);

    // A snapshot can be discarded when its iframe has no paint box. A later
    // snapshot must still carry the pending replacement for an already-sent key.
    let later = runtime.frame_paint_snapshots();
    assert!(later[0].image_updates.contains(&key));
    runtime.acknowledge_frame_image_updates(&[(frame.document, key.clone())]);
    assert!(
        !runtime.frame_paint_snapshots()[0]
            .image_updates
            .contains(&key)
    );
}
