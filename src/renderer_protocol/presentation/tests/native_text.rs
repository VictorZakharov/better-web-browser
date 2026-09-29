use super::*;

fn rejection() -> NativeTextRejection {
    NativeTextRejection {
        sequence: 7,
        generation: 2,
        target: DocumentNodeId::new((1_u128 << 64) | 9).unwrap(),
        value: "café".into(),
        selection_start: 4,
        selection_end: 4,
    }
}

#[test]
fn canceled_native_edit_round_trips_with_sequence_generation_and_selection() {
    let mut presentation = sample();
    presentation.runtime.native_text_rejection = Some(rejection());
    let decoded = RendererPresentation::decode(&presentation.encode().unwrap()).unwrap();
    assert_eq!(decoded.runtime.native_text_rejection, Some(rejection()));
    presentation
        .runtime
        .native_text_rejection
        .as_mut()
        .unwrap()
        .selection_end = 99;
    assert!(matches!(
        presentation.encode(),
        Err(ProtocolError::InvalidPayload("native text rejection"))
    ));
}

#[test]
fn one_native_rejection_survives_coalescing_but_two_cannot_merge() {
    let with_rejection = RuntimeReport {
        native_text_rejection: Some(rejection()),
        ..RuntimeReport::default()
    };
    let retained = with_rejection
        .clone()
        .coalesce(RuntimeReport::default())
        .unwrap();
    assert_eq!(retained.native_text_rejection, Some(rejection()));
    assert!(matches!(
        with_rejection.coalesce(retained),
        Err(ProtocolError::InvalidPayload(
            "coalesced native text rejections"
        ))
    ));
}

#[test]
fn cancellation_verdict_is_an_ordering_barrier_for_later_presentations() {
    let mut first = sample();
    first.runtime.native_text_rejection = Some(rejection());
    let mut next = sample();
    next.revision = first.revision + 1;
    let (retained, following) = first.coalesce(next).unwrap();
    assert_eq!(retained.runtime.native_text_rejection, Some(rejection()));
    assert!(
        following.is_some(),
        "later snapshot must not overtake rollback"
    );
}
