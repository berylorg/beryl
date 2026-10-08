use beryl_app::{
    composer_host::{
        ComposerHostFlushAdmission, ComposerHostFlushAdvance, ComposerHostFlushCapture,
        ComposerHostFlushPurpose, ComposerHostFlushState, ComposerHostPublicationCompletion,
    },
    composer_marker_seal::{DraftMarkerSealService, DraftMarkerSealServiceLimits},
};
use beryl_home_store::{CommandCancellation, HomeStore};
use beryl_model::SyndicThreadId;
use beryl_state::BerylState;
use std::num::NonZeroUsize;
use syndic_storage::{SyndicStorage, SyndicTimestamp};

#[path = "../syndic_composer_history/support.rs"]
mod editing;

pub fn publish_empty_after_typing(
    store: &HomeStore,
    state: &BerylState,
    syndic: &SyndicStorage,
    thread: SyndicThreadId,
) {
    let (mut host, empty) = editing::activated(syndic.clone(), store, thread, 200, 201);
    let typed = editing::commit_text(&mut host, store, empty, 1, 0, 0, "draft", 5, 1);
    let removed = editing::commit_text(&mut host, store, typed, 2, 0, 5, "", 0, 1);
    assert!(editing::candidate_text(syndic.clone(), store, removed).is_empty());
    assert!(removed.range_history_frontier().undo_available);
    let seals = DraftMarkerSealService::test_new(
        store,
        store.health().generation().unwrap(),
        syndic.clone(),
        state.assets(),
        DraftMarkerSealServiceLimits::new(NonZeroUsize::MIN, NonZeroUsize::MIN).unwrap(),
    )
    .unwrap();
    let flush = match host
        .begin_flush(ComposerHostFlushPurpose::ThreadSwitch)
        .unwrap()
    {
        ComposerHostFlushAdmission::Started { ticket, .. } => ticket,
        other => panic!("edited source flush must start: {other:?}"),
    };
    let capture = host
        .capture_flush_publication(
            store,
            flush,
            state.assets(),
            &seals,
            editing::operation_id(3),
            None,
            SyndicTimestamp::from_unix_millis(100),
            &CommandCancellation::new(),
        )
        .unwrap();
    assert!(
        matches!(capture, ComposerHostFlushCapture::Captured(_)),
        "typed and removed source must publish its authentic history: {capture:?}"
    );
    let mut saved = false;
    for _ in 0..256 {
        match host.advance_flush(store, flush).unwrap() {
            ComposerHostFlushAdvance::Progress(_) => {}
            other => panic!("source publication did not progress: {other:?}"),
        }
        if let Some(completion) = host.lifecycle_diagnostics().last_publication_completion() {
            assert_eq!(completion, ComposerHostPublicationCompletion::Published);
            saved = true;
            break;
        }
    }
    assert!(
        saved,
        "source publication did not settle within its bounded drive"
    );
    assert!(!host.is_dirty());
    assert!(matches!(
        host.capture_flush_publication(
            store,
            flush,
            state.assets(),
            &seals,
            editing::operation_id(5),
            None,
            SyndicTimestamp::from_unix_millis(100),
            &CommandCancellation::new(),
        )
        .unwrap(),
        ComposerHostFlushCapture::State(ComposerHostFlushState::DisposalRequired)
    ));
    host.capture_flush_disposal(
        store,
        flush,
        editing::operation_id(4),
        &CommandCancellation::new(),
    )
    .unwrap();
    assert!(matches!(
        host.advance_flush(store, flush).unwrap(),
        ComposerHostFlushAdvance::Satisfied(ComposerHostFlushPurpose::ThreadSwitch)
    ));
}
