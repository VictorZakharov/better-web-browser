use super::support::*;
use better_web_browser::engine::DisplayItem;
use better_web_browser::renderer_process::{RendererEvent, RendererSession};
use better_web_browser::renderer_protocol::{
    FetchResponseHead, FetchResponseResult, FetchResponseType, PresentationAcknowledgement,
    ResourceDestination, TransferChunk,
};
use std::time::Duration;

#[test]
fn contained_renderer_decodes_and_presents_video_without_browser_frame_ownership() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut launch = options();
    launch.enable_media = true;
    let mut session = RendererSession::launch(launch).expect("launch renderer and media worker");
    let document = better_web_browser::renderer_protocol::DocumentId::new(190).unwrap();
    let html = r#"<!doctype html><title>video</title>
        <video id="movie" src="/test.mp4" width="320" height="180" muted></video>
        <output id="state">waiting</output><script>
            movie.addEventListener('loadeddata', () => {
                movie.play().then(() => state.textContent = 'playing');
            });
        </script>"#;
    let body = html.as_bytes().to_vec();
    session
        .load_document(
            document_start(document, body.len()),
            empty_document_state(),
            body,
        )
        .unwrap();

    let mut request = None;
    let mut initial = None;
    while request.is_none() || initial.is_none() {
        match session.wait_for_event(Duration::from_secs(5)).unwrap() {
            RendererEvent::FetchBatch {
                document: actual,
                requests,
            } if actual == document => {
                assert_eq!(requests.len(), 1);
                let candidate = requests.into_iter().next().unwrap();
                assert_eq!(candidate.head.destination, ResourceDestination::Video);
                request = Some(candidate);
            }
            RendererEvent::Presentation(presentation) if presentation.document == document => {
                initial = Some(presentation);
            }
            RendererEvent::Diagnostic { .. } => {}
            event => panic!("unexpected renderer event before video response: {event:?}"),
        }
    }
    let initial = initial.unwrap();
    session
        .acknowledge_presentation(PresentationAcknowledgement {
            document,
            revision: initial.revision,
            presented: true,
            controls_applied: true,
        })
        .unwrap();

    let request = request.unwrap();
    let request_id = request.head.request_id;
    let bytes = decode_base64(include_str!("../fixtures/media/test-1s.mp4.base64"));
    let sink = session.fetch_response_sink(document);
    sink.start(FetchResponseHead {
        request_id,
        result: FetchResponseResult::Success {
            response_type: FetchResponseType::Basic,
            urls: vec![request.head.url],
            status: 200,
            headers: vec![
                ("content-type".into(), "video/mp4".into()),
                ("content-length".into(), bytes.len().to_string()),
            ],
        },
    })
    .unwrap();
    for (index, chunk) in bytes.chunks(64 * 1024).enumerate() {
        sink.chunk(TransferChunk {
            transfer_id: request_id,
            offset: (index * 64 * 1024) as u32,
            bytes: chunk.to_vec(),
        })
        .unwrap();
    }
    sink.end(request_id, bytes.len() as u32).unwrap();

    let rendered = loop {
        match session.wait_for_event(Duration::from_secs(10)).unwrap() {
            RendererEvent::Presentation(presentation) if presentation.document == document => {
                if presentation
                    .images
                    .iter()
                    .any(|image| image.url.starts_with("breeze-internal:media-frame:"))
                {
                    break presentation;
                }
            }
            RendererEvent::Diagnostic { .. } | RendererEvent::RuntimeUpdate(_) => {}
            event => panic!("unexpected renderer event while decoding video: {event:?}"),
        }
    };
    let frame = rendered
        .images
        .iter()
        .find(|image| image.url.starts_with("breeze-internal:media-frame:"))
        .unwrap();
    assert_eq!((frame.image.width, frame.image.height), (320, 240));
    let first_pixels = frame.image.bgra.clone();
    assert!(rendered.layout.items.iter().any(|item| {
        matches!(
            item,
            DisplayItem::Image { url, .. }
                if url.starts_with("breeze-internal:media-frame:")
        )
    }));
    session
        .acknowledge_presentation(PresentationAcknowledgement {
            document,
            revision: rendered.revision,
            presented: true,
            controls_applied: true,
        })
        .unwrap();
    session
        .advance_time(document, Duration::from_millis(250), 2)
        .expect("start playback and advance video presentation clock");
    let advanced = loop {
        match session.wait_for_event(Duration::from_secs(5)).unwrap() {
            RendererEvent::Presentation(presentation) if presentation.document == document => {
                if presentation
                    .images
                    .iter()
                    .find(|image| image.url.starts_with("breeze-internal:media-frame:"))
                    .is_some()
                {
                    break presentation;
                }
            }
            RendererEvent::Diagnostic { .. } | RendererEvent::RuntimeUpdate(_) => {}
            event => panic!("unexpected renderer event while advancing video: {event:?}"),
        }
    };
    let advanced_pixels = &advanced
        .images
        .iter()
        .find(|image| image.url.starts_with("breeze-internal:media-frame:"))
        .unwrap()
        .image
        .bgra;
    assert_ne!(
        advanced_pixels, &first_pixels,
        "video presentation did not advance"
    );
    assert!(
        advanced.layout.items.iter().any(|item| {
            matches!(item, DisplayItem::Text { text, .. } if text.contains("playing"))
        }),
        "the acknowledged play() promise did not settle"
    );
    session
        .shutdown()
        .expect("shutdown contained playback pair");
}

fn decode_base64(input: &str) -> Vec<u8> {
    let mut output = Vec::with_capacity(input.len() / 4 * 3);
    let mut quartet = [0_u8; 4];
    let mut count = 0;
    for byte in input.bytes().filter(|byte| !byte.is_ascii_whitespace()) {
        quartet[count] = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            b'=' => 64,
            _ => panic!("invalid base64 fixture byte"),
        };
        count += 1;
        if count == 4 {
            output.push((quartet[0] << 2) | (quartet[1] >> 4));
            if quartet[2] != 64 {
                output.push((quartet[1] << 4) | (quartet[2] >> 2));
            }
            if quartet[3] != 64 {
                output.push((quartet[2] << 6) | quartet[3]);
            }
            count = 0;
        }
    }
    assert_eq!(count, 0, "truncated base64 fixture");
    output
}
