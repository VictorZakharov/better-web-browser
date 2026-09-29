use super::*;
use crate::navigation::request::{FormPost, MAX_FORM_BODY_BYTES, NavigationOptions};

fn post() -> RuntimeReport {
    RuntimeReport {
        navigation_url: Some("https://example.test/submit".into()),
        navigation_options: NavigationOptions {
            target: "_self".into(),
            user_initiated: true,
            form_submission: true,
            noreferrer: true,
            replace_history: true,
            post: Some(FormPost {
                content_type: "text/plain".into(),
                body: b"q=hello\r\n".to_vec(),
            }),
        },
        ..Default::default()
    }
}

#[test]
fn history_traversal_ack_is_bounded_and_survives_wire_round_trip() {
    let mut presentation = sample();
    presentation.runtime.history_traversal_ack = Some(17);
    let decoded = RendererPresentation::decode(&presentation.encode().unwrap()).unwrap();
    assert_eq!(decoded.runtime.history_traversal_ack, Some(17));

    presentation.runtime.history_traversal_ack = Some(0);
    assert!(presentation.encode().is_err());
}

#[test]
fn history_update_scroll_snapshot_round_trips_and_rejects_invalid_offsets() {
    let mut presentation = sample();
    presentation.runtime.history_actions = vec![HistoryAction::Update {
        url: "https://example.test/#step".into(),
        replace: false,
        state: None,
        scroll_y: 120.25,
    }];
    let decoded = RendererPresentation::decode(&presentation.encode().unwrap()).unwrap();
    assert_eq!(
        decoded.runtime.history_actions,
        presentation.runtime.history_actions
    );
    for invalid in [f32::NAN, f32::INFINITY, -1.0] {
        presentation.runtime.history_actions = vec![HistoryAction::Update {
            url: "https://example.test/#step".into(),
            replace: false,
            state: None,
            scroll_y: invalid,
        }];
        assert!(presentation.encode().is_err());
    }
}

#[test]
fn navigation_payload_survives_wire_round_trip_and_unrelated_updates() {
    let mut presentation = sample();
    presentation.runtime = post().coalesce(RuntimeReport::default()).unwrap();
    let decoded = RendererPresentation::decode(&presentation.encode().unwrap()).unwrap();
    assert_eq!(decoded.runtime.navigation_url, post().navigation_url);
    assert_eq!(
        decoded.runtime.navigation_options,
        post().navigation_options
    );
    let get = decoded
        .runtime
        .coalesce(RuntimeReport {
            navigation_url: Some("https://example.test/get".into()),
            ..Default::default()
        })
        .unwrap();
    assert!(
        get.navigation_options.post.is_none(),
        "POST must not leak to a later GET"
    );
}

#[test]
fn malformed_form_payloads_are_rejected_on_both_sides_of_ipc() {
    for options in [
        NavigationOptions {
            target: "x".repeat(1025),
            ..Default::default()
        },
        NavigationOptions {
            post: Some(FormPost {
                content_type: "text/plain\r\nX-Injected: yes".into(),
                body: vec![],
            }),
            ..Default::default()
        },
        NavigationOptions {
            post: Some(FormPost {
                content_type: "text/plain".into(),
                body: vec![0; MAX_FORM_BODY_BYTES + 1],
            }),
            ..Default::default()
        },
    ] {
        let mut presentation = sample();
        presentation.runtime = post();
        presentation.runtime.navigation_options = options;
        assert!(presentation.encode().is_err());
    }
    use crate::renderer_protocol::wire::{WireReader, WireWriter};
    let mut wire = WireWriter::new();
    wire.u64(0);
    wire.u64(0);
    for _ in 0..3 {
        wire.u32(0);
    }
    wire.bool(true);
    wire.string("https://example.test/").unwrap();
    wire.string("_self").unwrap();
    wire.bool(false); // noreferrer
    wire.bool(false); // replacement history
    wire.bool(true);
    wire.bool(true);
    wire.bool(true);
    wire.string("text/plain\nInjected: yes").unwrap();
    wire.bytes(&[]).unwrap();
    assert!(codec::decode_runtime(&mut WireReader::new(&wire.finish())).is_err());
}
