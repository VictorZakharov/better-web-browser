use super::*;
use crate::engine::rendering::RefreshRate;

fn update(rate: RefreshRate) -> BrowserMessage {
    BrowserMessage::ViewportChanged {
        document: DocumentId::new(17).unwrap(),
        viewport: PresentedViewport {
            width: 1536.0,
            height: 694.4,
            style_width: 1536.0,
            dpi: 120,
            prefers_dark_color_scheme: false,
            refresh_rate: rate,
        },
    }
}

#[test]
fn display_update_preserves_fractional_and_high_refresh_rates_on_the_wire() {
    for (numerator, denominator) in [(60000, 1001), (144, 1), (240, 1), (500, 1), (u32::MAX, 1)] {
        let message = update(RefreshRate::new(numerator, denominator).unwrap());
        assert_eq!(
            FrameReader::new(Cursor::new(encoded_browser(&message)), session())
                .read_browser()
                .unwrap(),
            message
        );
    }
}

#[test]
fn invalid_or_truncated_display_rates_are_rejected_before_document_delivery() {
    for offset_from_end in [8, 4] {
        let mut bytes = encoded_browser(&update(RefreshRate::new(240, 1).unwrap()));
        let offset = bytes.len() - offset_from_end;
        bytes[offset..offset + 4].fill(0);
        assert!(matches!(
            FrameReader::new(Cursor::new(bytes), session()).read_browser(),
            Err(ProtocolError::InvalidPayload("display refresh rate"))
        ));
    }
    let mut bytes = encoded_browser(&update(RefreshRate::new(240, 1).unwrap()));
    bytes.pop();
    assert!(
        FrameReader::new(Cursor::new(bytes), session())
            .read_browser()
            .is_err()
    );
}
