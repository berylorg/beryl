use beryl_backend::{StreamedInputDescriptorKind, StreamedInputSourceError};

use super::{
    ProjectionCancellationToken,
    fixture::Fixture,
    support::drain_text,
};

#[test]
fn marker_free_factory_opens_independent_bounded_replay_cursors() {
    let fixture = Fixture::new(20);
    let expected = "αβγ accepted replay ".repeat(8_192);
    let record = fixture.accept_text(&expected);
    let cancellation = ProjectionCancellationToken::new();
    let factory = fixture
        .replay_factory(record.clone(), &cancellation)
        .unwrap();
    assert_eq!(factory.input_id(), record.id());
    let header = factory.header();
    assert_eq!(header.item_count(), 1);

    let mut first = factory.fresh_source();
    let mut second = factory.fresh_source();
    assert_eq!(first.header(), header);
    assert_eq!(second.header(), header);
    assert_eq!(
        first.begin_pass(&fixture.store, &cancellation).unwrap(),
        header
    );
    let first_descriptor = first
        .next_descriptor(&fixture.store, &cancellation)
        .unwrap()
        .unwrap();
    let StreamedInputDescriptorKind::Text(first_text) = first_descriptor.kind() else {
        panic!("marker-free accepted input must produce text")
    };
    let first_page = first
        .read_text_page(
            &fixture.store,
            &cancellation,
            first_text.source_id(),
            0,
            17,
        )
        .unwrap();
    assert!(first_page.next_offset().is_some());

    assert_eq!(
        second.begin_pass(&fixture.store, &cancellation).unwrap(),
        header
    );
    let second_descriptor = second
        .next_descriptor(&fixture.store, &cancellation)
        .unwrap()
        .unwrap();
    assert_eq!(first_descriptor, second_descriptor);
    let StreamedInputDescriptorKind::Text(second_text) = second_descriptor.kind() else {
        panic!("second fresh source must independently begin at text")
    };
    let second_value = drain_text(
        &mut second,
        &fixture,
        second_text.source_id(),
        0,
        257,
    );
    assert_eq!(second_value, expected);
    assert!(
        second
            .next_descriptor(&fixture.store, &cancellation)
            .unwrap()
            .is_none()
    );

    let first_tail = drain_text(
        &mut first,
        &fixture,
        first_text.source_id(),
        first_page.next_offset().unwrap(),
        257,
    );
    assert_eq!(format!("{}{first_tail}", first_page.text()), expected);
    assert!(
        first
            .next_descriptor(&fixture.store, &cancellation)
            .unwrap()
            .is_none()
    );

    let source_cancellation = ProjectionCancellationToken::new();
    source_cancellation.cancel();
    let mut cancelled_source = factory.fresh_source();
    assert!(matches!(
        cancelled_source.begin_pass(&fixture.store, &source_cancellation),
        Err(StreamedInputSourceError::Cancelled)
    ));
}
