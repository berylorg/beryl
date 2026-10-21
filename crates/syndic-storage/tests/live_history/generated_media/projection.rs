use super::*;
use syndic_storage::test_faults::{FixtureBatch, FixtureRecord};

pub(super) fn generated_item() -> SyndicItemId {
    SyndicItemId::from_bytes([20; 16])
}

pub(super) fn publish_projection(f: &MediaHome) -> (ItemProjectionSetRecord, ProjectionRecord) {
    crate::support::exact_cas::project_item_if_needed(
        &f.store,
        f.storage.clone(),
        generated_item(),
    );
    let head = f
        .storage
        .item_projection_head(&f.store, generated_item(), limit())
        .unwrap()
        .unwrap();
    let set = f
        .storage
        .item_projection_set(&f.store, generated_item(), head.generation(), limit())
        .unwrap()
        .unwrap();
    assert_eq!(
        set.source(),
        ItemProjectionSource::GeneratedResource(f.resource(20))
    );
    assert!(set.text_state().is_none());
    assert_eq!(set.projection_count(), 1);
    assert_eq!(set.resource_count(), 0);
    let members = f
        .storage
        .item_projections(
            &f.store,
            generated_item(),
            head.generation(),
            None,
            CursorReadLimits::new(256, 65_536).unwrap(),
        )
        .unwrap();
    assert_eq!(members.records().len(), 1);
    assert!(!members.has_more());
    let member = &members.records()[0];
    assert_eq!(member.ordinal().get(), 1);
    let projection = f
        .storage
        .projection(&f.store, member.projection_id(), limit())
        .unwrap()
        .unwrap();
    assert_eq!(
        projection.payload(),
        &(ProjectionPayload::GeneratedResource {
            resource_id: f.resource(20)
        })
    );
    assert!(projection.payload().source_range().is_none());
    assert!(projection.payload().block_id().is_none());
    assert!(
        f.storage
            .projection_resources(
                &f.store,
                projection.id(),
                None,
                CursorReadLimits::new(1, 65_536).unwrap()
            )
            .unwrap()
            .records()
            .is_empty()
    );
    (set, projection)
}

pub(super) fn end_and_freeze(f: &MediaHome) {
    admit_event(
        &f.store,
        f.storage.clone(),
        f.thread,
        f.turn,
        &f.source,
        SourceEventPayload::TurnEnded(
            TurnEndStatus::new(TurnTerminalOutcome::Complete, None).unwrap(),
        ),
        timestamp(8),
    );
    let user = turn_items(&f.store, &f.storage, f.turn)[0].clone();
    complete_item_frontier(
        &f.store,
        &f.storage,
        f.thread,
        f.turn,
        user.ordinal(),
        user.item_id(),
        timestamp(8),
    );
    let item = f
        .storage
        .canonical_item(&f.store, generated_item(), limit())
        .unwrap()
        .unwrap();
    let state = f
        .storage
        .turn_state(&f.store, f.turn, limit())
        .unwrap()
        .unwrap();
    assert_committed(execute(
        &f.store,
        f.storage.freeze_next_turn_item(
            f.storage.revision(&f.store).unwrap(),
            FreezeNextTurnItem::new(
                f.thread,
                f.turn,
                state.revision(),
                item.ordinal(),
                item.id(),
                timestamp(8),
            ),
        ),
    ));
}

#[test]
fn generated_projection_is_real_from_birth_and_reuses_identity_through_completion_freeze_and_admission()
 {
    let f = MediaHome::new("generated-projection-lifecycle");
    f.frame(
        20,
        false,
        ProviderImageGenerationStatusV1::InProgress,
        None,
        None,
        6,
    );
    let initial_proof = f.proof(20);
    assert!(initial_proof.item().projection_text_source().is_none());
    let (initial_set, initial_projection) = publish_projection(&f);
    let mut expected = Sha256::new();
    expected.update(b"beryl/syndic/generated-resource-projection/v1\0");
    expected.update([1]);
    expected.update(generated_item().as_bytes());
    expected.update(f.resource(20).as_bytes());
    assert_eq!(
        initial_projection.id().as_bytes(),
        &expected.finalize()[..16]
    );
    f.frame(
        20,
        true,
        ProviderImageGenerationStatusV1::Completed,
        None,
        Some(ProviderTextV1::inline("C:\\generated.png")),
        7,
    );
    assert_ne!(
        f.proof(20).source_fingerprint(),
        initial_proof.source_fingerprint()
    );
    assert_eq!(
        f.storage
            .item_projection_head(&f.store, generated_item(), limit())
            .unwrap()
            .unwrap()
            .lifecycle(),
        ProjectionLifecycle::Stale
    );
    let (completed_set, completed_projection) = publish_projection(&f);
    assert!(completed_set.source_item_revision() > initial_set.source_item_revision());
    assert_eq!(completed_projection, initial_projection);
    end_and_freeze(&f);
    assert_eq!(
        f.storage
            .item_projection_head(&f.store, generated_item(), limit())
            .unwrap()
            .unwrap()
            .lifecycle(),
        ProjectionLifecycle::Stale
    );
    let (frozen_set, frozen_projection) = publish_projection(&f);
    assert!(frozen_set.source_item_revision() > completed_set.source_item_revision());
    assert_eq!(frozen_projection, initial_projection);
    let asset = f.prepare(20);
    assert_committed(f.store.execute(f.publication(20, asset, 9)));
    let (admitted_set, admitted_projection) = publish_projection(&f);
    assert_eq!(admitted_set, frozen_set);
    assert_eq!(admitted_projection, initial_projection);
    assert_eq!(
        f.storage
            .turn_state(&f.store, f.turn, limit())
            .unwrap()
            .unwrap()
            .finalized_item_count(),
        2
    );
    assert_eq!(
        f.storage
            .item_projection_set(
                &f.store,
                generated_item(),
                initial_set.generation(),
                limit()
            )
            .unwrap(),
        Some(initial_set)
    );
    assert_eq!(
        f.storage
            .item_projection_set(
                &f.store,
                generated_item(),
                completed_set.generation(),
                limit()
            )
            .unwrap(),
        Some(completed_set)
    );
    converge_and_release_terminal_history(&f.store, f.storage.clone(), f.thread, f.turn);
    let tail = f
        .storage
        .thread_tail(&f.store, f.thread, limit())
        .unwrap()
        .unwrap();
    let child = id(70);
    assert_committed(execute(
        &f.store,
        f.storage.create_thread(
            f.storage.revision(&f.store).unwrap(),
            CreateThread::from_tail(
                child,
                draft_id(71),
                timestamp(10),
                DraftEditHistoryPolicyV1::new(65_536, 1).unwrap(),
                tail,
            )
            .unwrap(),
        ),
    ));
    crate::support::exact_cas::converge_transcript(&f.store, f.storage.clone(), child);
    let head = f
        .storage
        .transcript_view_head(&f.store, child, limit())
        .unwrap()
        .unwrap();
    let entries = f
        .storage
        .transcript_entries(
            &f.store,
            child,
            head.generation(),
            None,
            CursorReadLimits::new(256, 65_536).unwrap(),
        )
        .unwrap();
    let entry = entries
        .records()
        .iter()
        .find(|entry| entry.item_id() == generated_item())
        .unwrap();
    assert_eq!(entry.projection_id(), initial_projection.id());
    assert_eq!(entry.item_projection_generation(), frozen_set.generation());
    assert_eq!(f.proof(20).thread_id(), f.thread);
    f.store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
}

#[test]
fn generated_unavailable_resource_keeps_one_real_occurrence_and_structural_set() {
    let f = MediaHome::new("generated-projection-unavailable");
    f.frame(
        20,
        false,
        ProviderImageGenerationStatusV1::InProgress,
        None,
        None,
        6,
    );
    let (before, projection) = publish_projection(&f);
    assert_committed(f.unavailable(f.proof(20), Reason::LostSourceAuthority, 7));
    let (after, unavailable_projection) = publish_projection(&f);
    assert_eq!(after, before);
    assert_eq!(unavailable_projection, projection);
    crate::support::exact_cas::project_item_if_needed(
        &f.store,
        f.storage.clone(),
        turn_items(&f.store, &f.storage, f.turn)[0].item_id(),
    );
    crate::support::exact_cas::converge_transcript(&f.store, f.storage.clone(), f.thread);
    let head = f
        .storage
        .transcript_view_head(&f.store, f.thread, limit())
        .unwrap()
        .unwrap();
    let entries = f
        .storage
        .transcript_entries(
            &f.store,
            f.thread,
            head.generation(),
            None,
            CursorReadLimits::new(256, 65_536).unwrap(),
        )
        .unwrap();
    assert!(
        entries
            .records()
            .iter()
            .any(|entry| entry.projection_id() == projection.id())
    );
    assert_eq!(
        f.storage
            .turn_state(&f.store, f.turn, limit())
            .unwrap()
            .unwrap()
            .finalized_item_count(),
        0
    );
    assert!(
        !f.storage
            .history_summary(&f.store, f.thread, limit())
            .unwrap()
            .unwrap()
            .complete()
    );
    f.store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
}

#[test]
fn asset_resolution_leaves_frozen_frontier_behind_for_absent_or_stale_generated_sets() {
    for stale_set in [false, true] {
        let f = MediaHome::new(&format!(
            "generated-projection-required-frontier-{stale_set}"
        ));
        f.frame(
            20,
            true,
            ProviderImageGenerationStatusV1::Completed,
            None,
            Some(ProviderTextV1::inline("C:\\generated.png")),
            6,
        );
        if stale_set {
            publish_projection(&f);
        }
        end_and_freeze(&f);
        let head = f
            .storage
            .item_projection_head(&f.store, generated_item(), limit())
            .unwrap();
        assert_eq!(
            head.as_ref().map(|head| head.lifecycle()),
            stale_set.then_some(ProjectionLifecycle::Stale)
        );
        let asset = f.prepare(20);
        assert_committed(f.store.execute(f.publication(20, asset, 9)));
        let before = f
            .storage
            .turn_state(&f.store, f.turn, limit())
            .unwrap()
            .unwrap();
        assert_eq!(before.finalized_item_count(), 1);
        let item = f.proof(20).item().clone();
        let request = FinalizeNextTurnItem::new(
            f.thread,
            f.turn,
            before.revision(),
            item.ordinal(),
            item.id(),
            timestamp(9),
        );
        assert!(matches!(
            f.store
                .execute_current(f.storage.current_finalize_next_turn_item(request)),
            CommandOutcome::NotCommitted { .. }
        ));
        publish_projection(&f);
        assert_committed(
            f.store
                .execute_current(f.storage.current_finalize_next_turn_item(
                    FinalizeNextTurnItem::new(
                        f.thread,
                        f.turn,
                        before.revision(),
                        item.ordinal(),
                        item.id(),
                        timestamp(9),
                    ),
                )),
        );
        assert_eq!(
            f.storage
                .turn_state(&f.store, f.turn, limit())
                .unwrap()
                .unwrap()
                .finalized_item_count(),
            2
        );
        f.store
            .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
            .unwrap();
    }
}

#[test]
fn generated_projection_publication_refuses_wrong_source_revision_and_generation() {
    let f = MediaHome::new("generated-projection-guards");
    f.frame(
        20,
        false,
        ProviderImageGenerationStatusV1::InProgress,
        None,
        None,
        6,
    );
    let item = f.proof(20).item().clone();
    for request in [
        PublishGeneratedResourceProjection::new(
            item.id(),
            item.revision(),
            beryl_model::SyndicResourceId::from_bytes([99; 16]),
            ItemProjectionGeneration::FIRST,
        ),
        PublishGeneratedResourceProjection::new(
            item.id(),
            item.revision().checked_next().unwrap(),
            f.resource(20),
            ItemProjectionGeneration::FIRST,
        ),
        PublishGeneratedResourceProjection::new(
            item.id(),
            item.revision(),
            f.resource(20),
            ItemProjectionGeneration::new(2).unwrap(),
        ),
    ] {
        assert!(matches!(
            f.store.execute_current(
                f.storage
                    .current_publish_generated_resource_projection(request)
            ),
            CommandOutcome::NotCommitted { .. }
        ));
    }
    assert!(matches!(
        f.store
            .execute_current(f.storage.current_start_item_projection_build(
                StartItemProjectionBuild::new(
                    item.id(),
                    item.revision(),
                    ItemProjectionGeneration::FIRST
                )
            )),
        CommandOutcome::NotCommitted { .. }
    ));
    let (set, projection) = publish_projection(&f);
    assert!(matches!(
        f.store
            .execute_current(f.storage.current_publish_generated_resource_projection(
                PublishGeneratedResourceProjection::new(
                    item.id(),
                    item.revision(),
                    f.resource(20),
                    set.generation().checked_next().unwrap()
                )
            )),
        CommandOutcome::NotCommitted { .. }
    ));
    assert_eq!(publish_projection(&f), (set, projection));
    f.store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
}

#[test]
fn whole_home_rejects_generated_projection_resource_substitution() {
    let f = MediaHome::new("generated-projection-substitution");
    f.frame(
        20,
        false,
        ProviderImageGenerationStatusV1::InProgress,
        None,
        None,
        6,
    );
    let (_, projection) = publish_projection(&f);
    let substituted = ProjectionRecord::new(
        projection.id(),
        projection.revision(),
        projection.item_id(),
        projection.turn_id(),
        projection.ordinal(),
        ProjectionPayload::GeneratedResource {
            resource_id: beryl_model::SyndicResourceId::from_bytes([99; 16]),
        },
    );
    let mut fixture = FixtureBatch::new();
    fixture.put(FixtureRecord::Projection(substituted)).unwrap();
    assert_committed(execute(
        &f.store,
        f.storage
            .fixture_contribution(f.storage.revision(&f.store).unwrap(), fixture),
    ));
    assert!(
        f.store
            .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
            .is_err()
    );
}
