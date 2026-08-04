use super::*;

pub(in super::super) fn read_provider_frame(
    home: &HomeStore,
    storage: SyndicStorage,
    reference: &syndic_storage::SealedProviderFrameReference,
) -> ProviderItemFrameV1 {
    let mut content = Vec::new();
    let mut after = None;
    loop {
        let page = storage
            .content_chunks(
                home,
                reference.content().id(),
                after,
                CursorReadLimits::new(32, POINT_READ_BYTES).unwrap(),
            )
            .unwrap();
        for chunk in page.records() {
            content.extend_from_slice(chunk.bytes());
            after = Some(chunk.ordinal());
        }
        if !page.has_more() {
            break;
        }
    }
    let start = usize::try_from(reference.frame().encoded_start()).unwrap();
    let end = usize::try_from(reference.frame().encoded_end()).unwrap();
    syndic_storage::decode_bounded_provider_item_frame_v1(
        &content[start..end],
        syndic_storage::PROVIDER_FRAME_BOUNDED_DECODE_MAX_BYTES,
        reference.frame().encoded_start(),
    )
    .unwrap()
}

pub(in super::super) fn assert_user_message_frame(
    frame: &ProviderItemFrameV1,
    expected_ordinal: ProviderFrameOrdinalV1,
    expected_lifecycle: UserMessageEchoLifecycle,
    expected_item_id: &CasItemId,
    expected_content: ContentReference,
) {
    assert_eq!(frame.ordinal(), expected_ordinal);
    assert_eq!(frame.item_id(), expected_item_id);
    let (observed_at, item) = match (expected_lifecycle, frame.observation()) {
        (
            UserMessageEchoLifecycle::Started,
            ProviderItemObservationV1::Started { observed_at, item },
        )
        | (
            UserMessageEchoLifecycle::Completed,
            ProviderItemObservationV1::Completed { observed_at, item },
        ) => (*observed_at, item),
        (_, observation) => panic!("unexpected checked-user observation: {observation:?}"),
    };
    assert_eq!(
        observed_at,
        ProviderLifecycleTimestampMsV1::new(match expected_lifecycle {
            UserMessageEchoLifecycle::Started => 10,
            UserMessageEchoLifecycle::Completed => 11,
        })
    );
    let ProviderItemV1::UserMessage(message) = item else {
        panic!("checked-user frame did not retain UserMessage")
    };
    assert!(message.client_id.is_none());
    assert_eq!(message.submitted.content, expected_content);
}
