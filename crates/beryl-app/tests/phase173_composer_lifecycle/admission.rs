use beryl_app::composer_host::{
    ComposerHostError, ComposerHostFlushAdmission, ComposerHostFlushAdvance,
    ComposerHostFlushCapture, ComposerHostFlushPurpose, ComposerHostFlushState,
    ComposerHostMutationOutcome,
};
use beryl_home_store::CommandCancellation;
use beryl_state::BerylState;
use gpui_text_input::{
    BindingId, LogicalExtent, MutationCommitRequest, MutationCursor, MutationFinishInput,
    MutationIdentity, MutationKey, MutationKind, MutationLane, MutationPage, MutationPageItem,
    MutationPageKey, MutationPageRequest, MutationPositions, MutationStreamFinish, MutationTotals,
    OperationId, SourceRevision,
};
use syndic_storage::SyndicTimestamp;

use super::{base, composer, publication, started_flush};

const DISPOSING_PURPOSES: [ComposerHostFlushPurpose; 4] = [
    ComposerHostFlushPurpose::ThreadSwitch,
    ComposerHostFlushPurpose::WindowClose,
    ComposerHostFlushPurpose::ApplicationExit,
    ComposerHostFlushPurpose::Release,
];

#[test]
fn every_disposing_barrier_freezes_new_edit_undo_and_redo_admission() {
    for (index, purpose) in DISPOSING_PURPOSES.into_iter().enumerate() {
        for dirty in [false, true] {
            let seed = 100_u8
                .wrapping_add((index as u8).wrapping_mul(20))
                .wrapping_add(u8::from(dirty).wrapping_mul(8));
            let (_home, store, storage, thread) =
                base::fixture("phase173-disposing-admission-freeze", seed);
            let (mut host, empty) =
                composer::activated(storage, &store, thread, seed + 1, seed + 2);
            let binding = if dirty {
                composer::commit_text(&mut host, &store, empty, 1, 0, 0, "a", 1, 1)
            } else {
                empty
            };
            let (flush, _) = started_flush(&mut host, purpose);
            assert_eq!(host.binding(), Some(binding));
            assert!(matches!(
                composer::begin_text(&mut host, &store, binding, 2, 0),
                Err(ComposerHostError::LifecycleBlocked)
            ));
            for (operation, kind) in [(3, MutationKind::Undo), (4, MutationKind::Redo)] {
                let intent = composer::history_intent(
                    binding,
                    operation,
                    kind,
                    composer::position(binding.logical_extent().logical_utf8_bytes()),
                );
                assert!(matches!(
                    host.begin_history_selection(&store, binding, intent),
                    Err(ComposerHostError::LifecycleBlocked)
                ));
            }
            assert_eq!(host.binding(), Some(binding));
            assert_eq!(host.lifecycle_diagnostics().barriers(), 1);
            assert!(host.flush_state(flush).is_ok());
        }
    }
}

#[test]
fn submission_allows_edits_until_a_disposing_join_upgrades_the_barrier() {
    let (_home, store, storage, thread) = base::fixture("phase173-submission-upgrade-freeze", 190);
    let (mut host, empty) = composer::activated(storage, &store, thread, 191, 192);
    let (flush, _) = started_flush(&mut host, ComposerHostFlushPurpose::Submission);
    let edited = composer::commit_text(&mut host, &store, empty, 1, 0, 0, "a", 1, 1);
    assert!(matches!(
        host.begin_flush(ComposerHostFlushPurpose::ApplicationExit)
            .unwrap(),
        ComposerHostFlushAdmission::Joined { ticket, .. } if ticket == flush
    ));
    assert!(matches!(
        composer::begin_text(&mut host, &store, edited, 2, 1),
        Err(ComposerHostError::LifecycleBlocked)
    ));
    for (operation, kind) in [(3, MutationKind::Undo), (4, MutationKind::Redo)] {
        let intent = composer::history_intent(edited, operation, kind, composer::position(1));
        assert!(matches!(
            host.begin_history_selection(&store, edited, intent),
            Err(ComposerHostError::LifecycleBlocked)
        ));
    }
    assert_eq!(host.binding(), Some(edited));
}

#[test]
fn work_admitted_before_a_disposing_barrier_finishes_and_the_barrier_drains_it() {
    let (_home, mut store, storage, thread) = base::fixture("phase173-admitted-work-drains", 200);
    let assets = BerylState::register(&mut store).unwrap().assets();
    let seals = publication::service(&store, storage, assets, 1, 1);
    let (mut host, empty) = composer::activated(storage, &store, thread, 201, 202);
    let dirty = composer::commit_text(&mut host, &store, empty, 1, 0, 0, "a", 1, 1);
    composer::begin_text(&mut host, &store, dirty, 2, 1).unwrap();
    let (flush, _) = started_flush(&mut host, ComposerHostFlushPurpose::Release);
    let admitted = finish_begun_text(&mut host, &store, dirty, 2, "b");
    let publication = match host
        .capture_flush_publication(
            &store,
            flush,
            assets,
            &seals,
            composer::operation_id(3),
            None,
            SyndicTimestamp::from_unix_millis(3),
            &CommandCancellation::new(),
        )
        .unwrap()
    {
        ComposerHostFlushCapture::Captured(ticket) => ticket,
        ComposerHostFlushCapture::State(ComposerHostFlushState::DisposalRequired) => {
            panic!("undo unexpectedly required no exact publication: {flush:?}")
        }
        other => panic!("already-admitted work was not captured: {other:?}"),
    };
    assert_eq!(
        host.advance_flush(&store, flush).unwrap(),
        ComposerHostFlushAdvance::Progress(ComposerHostFlushState::DisposalRequired)
    );
    assert_eq!(host.binding().unwrap().root(), admitted.root());
    assert_eq!(host.publication_custody_count(), 0);
    assert_eq!(
        host.advance_autosave(&store, publication).unwrap(),
        beryl_app::composer_host::ComposerHostAutosaveAdvance::Stale
    );
    assert!(matches!(
        host.capture_flush_disposal(
            &store,
            flush,
            composer::operation_id(4),
            &CommandCancellation::new(),
        )
        .unwrap(),
        ComposerHostFlushCapture::State(ComposerHostFlushState::DisposalRequired)
    ));
    assert_eq!(
        host.advance_flush(&store, flush).unwrap(),
        ComposerHostFlushAdvance::Satisfied(ComposerHostFlushPurpose::Release)
    );
    assert!(host.binding().is_none());
}

fn finish_begun_text(
    host: &mut beryl_app::composer_host::SyndicComposerHost,
    store: &beryl_home_store::HomeStore,
    binding: beryl_app::composer_host::ComposerHostBinding,
    operation: u64,
    text: &str,
) -> beryl_app::composer_host::ComposerHostBinding {
    let key = MutationKey::new(
        BindingId::new(binding.host_generation().get()),
        SourceRevision::new(binding.candidate().candidate_generation()),
        OperationId::new(operation),
    );
    let page = MutationPage::new(
        MutationPageKey::new(
            key,
            MutationLane::Proposal,
            MutationCursor::new(0),
            0,
            MutationIdentity::ROOT,
        ),
        MutationCursor::new(1),
        vec![MutationPageItem::Utf8 {
            inserted_offset: 0,
            text: text.into(),
        }],
    )
    .unwrap();
    let proposal_finish = MutationStreamFinish {
        next_cursor: page.next_cursor(),
        next_ordinal: 1,
        cumulative_identity: page.cumulative_identity(),
        totals: page.totals(),
    };
    host.stage_mutation_page(store, MutationPageRequest::new(page), Box::new([]))
        .unwrap();
    host.finish_mutation_input(
        store,
        MutationFinishInput::new(
            key,
            MutationStreamFinish {
                next_cursor: MutationCursor::new(0),
                next_ordinal: 0,
                cumulative_identity: MutationIdentity::ROOT,
                totals: MutationTotals::default(),
            },
            proposal_finish,
            LogicalExtent::new(2, 1),
            MutationPositions::collapsed(composer::position(2)),
        ),
    )
    .unwrap();
    for _ in 0..16 {
        match host.execute_mutation(
            store,
            MutationCommitRequest::new(key, MutationIdentity::ROOT),
            &CommandCancellation::new(),
        ) {
            Ok(ComposerHostMutationOutcome::Committed { binding, .. }) => return binding,
            Err(ComposerHostError::MutationWorkPending) => {}
            other => panic!("already admitted edit did not finish: {other:?}"),
        }
    }
    panic!("already admitted edit remained pending")
}
