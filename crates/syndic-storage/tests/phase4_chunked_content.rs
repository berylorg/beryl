#![cfg(feature = "test-faults")]

mod support;

use beryl_home_store::{
    CommandError, CommandOutcome, CommitReceipt, CursorReadLimits, HomeCommand, HomeOpenOptions,
    HomeSchemaVersion, HomeStore,
};
use beryl_model::{
    AcceptedInputRevision, ContentRevision, DraftRevision, ImageLabelOrdinal, InputGateRevision,
    ProjectionRevision, SyndicAcceptedInputId, SyndicDraftId, SyndicDraftMarkerId, SyndicItemId,
    SyndicTurnId, ThreadRevision, advance_sequential_marker_digest, sequential_marker_digest_seed,
};
use syndic_storage::test_faults::FixtureRecord;
use syndic_storage::{
    AcceptedInputAdmissionProof, AcceptedInputLifecycle, AcceptedInputOrdinal, AcceptedInputRecord,
    AcceptedNextSourceRecord, AcceptedOrderIndexRecord, AcceptedRouteGeneration,
    AcceptedRouteGenerationRecord, AcceptedRouteLeafRecord, AcceptedRouteLeafState,
    AcceptedRouteRevision, AcceptedRouteTarget, AdvanceItemProjectionBuild,
    CONTENT_APPEND_MAX_CHUNKS, CONTENT_CHUNK_MAX_BYTES, CanonicalItemRecord, ComposerAtom,
    ComposerContentAssembler, ComposerPayload, ContentAppend, ContentBuild, ContentLifecycle,
    ContentManifestRecord, DraftByThreadRecord, HistorySummaryRecord, InputGateRecord,
    InputGateState, ItemProjectionGeneration, NextTurnReason, PreparedContent, SelectedPathProof,
    StartItemProjectionBuild, SyndicMutationError, SyndicPointReadLimit, SyndicStorage,
    ThreadRecord, TurnDepth, TurnEndStatus, TurnIncompleteReason, TurnItemIndexRecord,
    TurnItemOrdinal, TurnKind, TurnLifecycle, TurnRecord, TurnStateRecord, TurnStateRevision,
    TurnTerminalOutcome,
};

use support::*;

fn point_limit() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(1_000_000).unwrap()
}

fn execute(
    store: &HomeStore,
    storage: SyndicStorage,
    contribution: beryl_home_store::MutationContribution,
) -> CommitReceipt {
    match execute_outcome(store, contribution) {
        CommandOutcome::Committed {
            receipt,
            later_failure: None,
        } => {
            assert!(
                storage
                    .committed_revision(store, &receipt)
                    .unwrap()
                    .is_some()
            );
            receipt
        }
        outcome => panic!("expected clean chunked-content command, got {outcome:?}"),
    }
}

fn execute_outcome(
    store: &HomeStore,
    contribution: beryl_home_store::MutationContribution,
) -> CommandOutcome {
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command.add(contribution).unwrap();
    store.execute(command)
}

fn project_item(store: &HomeStore, storage: SyndicStorage, item: SyndicItemId) {
    let canonical = storage
        .canonical_item(store, item, point_limit())
        .unwrap()
        .unwrap();
    let generation = ItemProjectionGeneration::FIRST;
    execute(
        store,
        storage,
        storage.start_item_projection_build(
            storage.revision(store).unwrap(),
            StartItemProjectionBuild::new(item, canonical.revision(), generation),
        ),
    );
    loop {
        if storage
            .item_projection_set(store, item, generation, point_limit())
            .unwrap()
            .is_some()
        {
            return;
        }
        let build = storage
            .item_projection_build(store, item, generation, point_limit())
            .unwrap()
            .unwrap();
        execute(
            store,
            storage,
            storage.advance_item_projection_build(
                storage.revision(store).unwrap(),
                AdvanceItemProjectionBuild::new(item, generation, build.revision()),
            ),
        );
    }
}

fn append_one_batch(
    store: &HomeStore,
    storage: SyndicStorage,
    manifest: &ContentManifestRecord,
    content: &PreparedContent,
) -> Option<ContentManifestRecord> {
    let append = ContentAppend::prepare(manifest, content).unwrap()?;
    let next = append.next_manifest().clone();
    let advanced = next.chunk_count() - manifest.chunk_count();
    assert!(advanced > 0);
    assert!(advanced <= u64::try_from(CONTENT_APPEND_MAX_CHUNKS).unwrap());
    execute(
        store,
        storage,
        storage.append_content(storage.revision(store).unwrap(), append),
    );
    Some(next)
}

fn seal_prepared_content(
    store: &HomeStore,
    storage: SyndicStorage,
    manifest: &ContentManifestRecord,
    content: &PreparedContent,
) -> ContentManifestRecord {
    assert_eq!(manifest.lifecycle(), ContentLifecycle::Building);
    assert_eq!(manifest.chunk_count(), content.summary().chunk_count());
    assert_eq!(manifest.encoded_bytes(), content.summary().encoded_bytes());
    assert_eq!(manifest.chain_digest(), content.summary().digest());
    let sealed = content.sealed_manifest(manifest.revision().checked_next().unwrap());
    commit(
        store,
        storage,
        batch([FixtureRecord::ContentManifest(sealed.clone())]),
    );
    sealed
}

fn huge_boundary_payload() -> ComposerPayload {
    let mut boundary = "a".repeat(CONTENT_CHUNK_MAX_BYTES - 19);
    boundary.push('🧵');
    let large = "word ".repeat(2_000_000);
    ComposerPayload::new(vec![
        ComposerAtom::text(boundary).unwrap(),
        ComposerAtom::text(large).unwrap(),
    ])
    .unwrap()
}

#[path = "phase4_chunked_content/identity_mismatch.rs"]
mod identity_mismatch;
#[path = "phase4_chunked_content/large_draft.rs"]
mod large_draft;
#[path = "phase4_chunked_content/marker_summary.rs"]
mod marker_summary;
#[path = "phase4_chunked_content/owner_metadata.rs"]
mod owner_metadata;
