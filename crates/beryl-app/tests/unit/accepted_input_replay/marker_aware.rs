use beryl_backend::{StreamedInputDescriptorKind, StreamedInputSourceError};
use beryl_model::RuntimeMode;
use beryl_state::{AssetOwner, AssetOwnerHeadUpdate, UpdateAssetOwnerHeads};

use super::{
    AcceptedInputReplayContext, AcceptedInputReplayError,
    AcceptedInputReplayFactory, ProjectionCancellationToken,
    fixture::{Fixture, execute_one},
    support::drain_text,
};

#[test]
fn repeated_image_labels_and_large_text_replay_in_order_with_bounded_pages() {
    let fixture = Fixture::new(60);
    let leading = "large accepted text αβγ ".repeat(6_000);
    let record = fixture.accept_repeated_image(&leading);
    let cancellation = ProjectionCancellationToken::new();
    let context = AcceptedInputReplayContext::new(
        fixture.store.home_id(),
        fixture.store.health().generation().unwrap(),
        RuntimeMode::host(),
    );

    assert!(matches!(
        AcceptedInputReplayFactory::prepare(
            &fixture.store,
            fixture.storage,
            fixture.state.assets(),
            context.clone(),
            record.clone(),
            None,
            &cancellation,
        ),
        Err(AcceptedInputReplayError::AssetReferenceSetMismatch)
    ));

    let owner_head = fixture
        .state
        .assets()
        .owner_head(&fixture.store, AssetOwner::AcceptedInput(record.id()))
        .unwrap();
    let exact_owner_head = owner_head.clone().unwrap();
    let factory = AcceptedInputReplayFactory::prepare(
        &fixture.store,
        fixture.storage,
        fixture.state.assets(),
        context,
        record,
        owner_head,
        &cancellation,
    )
    .unwrap();
    assert_eq!(factory.header().item_count(), 3);
    let mut source = factory.fresh_source();
    source.begin_pass(&fixture.store, &cancellation).unwrap();

    let first = source
        .next_descriptor(&fixture.store, &cancellation)
        .unwrap()
        .unwrap();
    assert_eq!(first.item_ordinal(), 1);
    let StreamedInputDescriptorKind::Text(first_text) = first.kind() else {
        panic!("large leading run must be text")
    };
    let first_value = drain_text(
        &mut source,
        &fixture,
        first_text.source_id(),
        0,
        101,
    );
    assert!(
        first_value.starts_with(&leading),
        "leading authored text changed: expected at least {} bytes, got {}",
        leading.len(),
        first_value.len()
    );
    assert_eq!(&first_value[leading.len()..], "Image A:");

    let image = source
        .next_descriptor(&fixture.store, &cancellation)
        .unwrap()
        .unwrap();
    assert_eq!(image.item_ordinal(), 2);
    let StreamedInputDescriptorKind::LocalImage(image) = image.kind() else {
        panic!("the first label occurrence must emit one local image")
    };
    assert!(!image.path().is_empty());

    let tail = source
        .next_descriptor(&fixture.store, &cancellation)
        .unwrap()
        .unwrap();
    assert_eq!(tail.item_ordinal(), 3);
    let StreamedInputDescriptorKind::Text(tail_text) = tail.kind() else {
        panic!("the repeated label must remain in the following text run")
    };
    assert_eq!(
        drain_text(
            &mut source,
            &fixture,
            tail_text.source_id(),
            0,
            101,
        ),
        " between [Image A] after"
    );
    assert!(
        source
            .next_descriptor(&fixture.store, &cancellation)
            .unwrap()
            .is_none()
    );

    let assets = fixture.state.assets();
    execute_one(
        &fixture.store,
        assets.update_owner_heads(
            assets.revision(&fixture.store).unwrap(),
            UpdateAssetOwnerHeads::new(
                vec![AssetOwnerHeadUpdate::replace(
                    exact_owner_head.owner(),
                    Some(exact_owner_head.expectation()),
                    None,
                )]
                .into_boxed_slice(),
            )
            .unwrap(),
        ),
    );
    let mut drifted_source = factory.fresh_source();
    assert!(matches!(
        drifted_source.begin_pass(&fixture.store, &cancellation),
        Err(StreamedInputSourceError::InvalidSource)
    ));
}

#[test]
fn marker_aware_source_rechecks_owner_before_emitting_cached_image() {
    let fixture = Fixture::new(61);
    let record = fixture.accept_repeated_image("before ");
    let cancellation = ProjectionCancellationToken::new();
    let owner_head = fixture
        .state
        .assets()
        .owner_head(&fixture.store, AssetOwner::AcceptedInput(record.id()))
        .unwrap()
        .unwrap();
    let factory = AcceptedInputReplayFactory::prepare(
        &fixture.store,
        fixture.storage,
        fixture.state.assets(),
        AcceptedInputReplayContext::new(
            fixture.store.home_id(),
            fixture.store.health().generation().unwrap(),
            RuntimeMode::host(),
        ),
        record,
        Some(owner_head.clone()),
        &cancellation,
    )
    .unwrap();
    let mut source = factory.fresh_source();
    source.begin_pass(&fixture.store, &cancellation).unwrap();
    let leading = source
        .next_descriptor(&fixture.store, &cancellation)
        .unwrap()
        .unwrap();
    let StreamedInputDescriptorKind::Text(text) = leading.kind() else {
        panic!("the leading run must be text")
    };
    assert_eq!(
        drain_text(&mut source, &fixture, text.source_id(), 0, 101),
        "before Image A:"
    );

    let assets = fixture.state.assets();
    execute_one(
        &fixture.store,
        assets.update_owner_heads(
            assets.revision(&fixture.store).unwrap(),
            UpdateAssetOwnerHeads::new(
                vec![AssetOwnerHeadUpdate::replace(
                    owner_head.owner(),
                    Some(owner_head.expectation()),
                    None,
                )]
                .into_boxed_slice(),
            )
            .unwrap(),
        ),
    );

    assert!(matches!(
        source.next_descriptor(&fixture.store, &cancellation),
        Err(StreamedInputSourceError::InvalidSource)
    ));
}
