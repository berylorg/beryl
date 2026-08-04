use beryl_home_store::HomeStore;
use beryl_model::{CasItemId, SyndicThreadId, SyndicTurnId};
use sha2::{Digest, Sha256};
use syndic_storage::{
    CasItemSource, CasTurnSource, SourceEventPayload, SourceEventSequence, SyndicStorage,
};

use super::{
    server::{ObservationSpec, SEMANTIC_PATTERN},
    syndic::point_limit,
};

const TEXT_PAGE_BYTES: usize = 65_536;

pub fn assert_atomic_frontier(
    store: &HomeStore,
    storage: SyndicStorage,
    thread: SyndicThreadId,
    turn: SyndicTurnId,
    expected_provider_events: u64,
) {
    let expected_source_events = expected_provider_events.checked_add(1).unwrap();
    let state = storage
        .turn_state(store, turn, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(state.source_event_count(), expected_source_events);
    let activation = storage
        .source_event(
            store,
            turn,
            SourceEventSequence::new(1).unwrap(),
            point_limit(),
        )
        .unwrap()
        .unwrap();
    assert!(matches!(
        activation.payload(),
        SourceEventPayload::TurnActivated
    ));
    for sequence in 2..=expected_source_events {
        let event = storage
            .source_event(
                store,
                turn,
                SourceEventSequence::new(sequence).unwrap(),
                point_limit(),
            )
            .unwrap()
            .unwrap();
        assert!(matches!(
            event.payload(),
            SourceEventPayload::ItemFrame { .. }
        ));
    }
    let head = storage
        .activity_query_head(store, thread, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(head.source_frontier(), expected_source_events);
    assert_eq!(head.logical_row_count(), 0);
    assert_eq!(head.running_row_count(), 0);
}

pub fn assert_item_digest(
    store: &HomeStore,
    storage: SyndicStorage,
    source: &CasTurnSource,
    spec: ObservationSpec,
) {
    let item_source = CasItemSource::new(source.clone(), CasItemId::new(spec.item_id()).unwrap());
    let capture = storage
        .capture_item(store, &item_source, point_limit())
        .unwrap()
        .unwrap();
    let mut digest = Sha256::new();
    let mut offset = 0;
    loop {
        let page = storage
            .capture_item_text_range(store, &capture, offset, TEXT_PAGE_BYTES, point_limit())
            .unwrap();
        digest.update(page.text().as_bytes());
        let Some(next) = page.next_offset() else {
            offset = offset
                .checked_add(u64::try_from(page.text().len()).unwrap())
                .unwrap();
            break;
        };
        assert!(page.text().len() <= TEXT_PAGE_BYTES);
        offset = next;
    }
    assert_eq!(offset, spec.semantic_bytes());
    let actual: [u8; 32] = digest.finalize().into();
    assert_eq!(actual, expected_digest(spec));
}

pub fn assert_item_absent(
    store: &HomeStore,
    storage: SyndicStorage,
    source: &CasTurnSource,
    sequence: u64,
) {
    let item_source = CasItemSource::new(
        source.clone(),
        CasItemId::new(format!("phase35-item-{sequence}")).unwrap(),
    );
    assert!(
        storage
            .capture_item(store, &item_source, point_limit())
            .unwrap()
            .is_none()
    );
}

fn expected_digest(spec: ObservationSpec) -> [u8; 32] {
    const PATTERNS_PER_PAGE: usize = 1_024;
    let mut page = [0_u8; SEMANTIC_PATTERN.len() * PATTERNS_PER_PAGE];
    for chunk in page.chunks_exact_mut(SEMANTIC_PATTERN.len()) {
        chunk.copy_from_slice(SEMANTIC_PATTERN);
    }
    let mut remaining = spec.pattern_repetitions;
    let mut digest = Sha256::new();
    while remaining >= u64::try_from(PATTERNS_PER_PAGE).unwrap() {
        digest.update(page);
        remaining -= u64::try_from(PATTERNS_PER_PAGE).unwrap();
    }
    digest.update(&page[..usize::try_from(remaining).unwrap() * SEMANTIC_PATTERN.len()]);
    digest.finalize().into()
}
