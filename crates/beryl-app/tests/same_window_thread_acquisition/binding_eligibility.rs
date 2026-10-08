use super::*;
use syndic_storage::{BindingLifecycle, BindingState};

fn set_binding(fixture: &Fixture, seed: u8, lifecycle: BindingLifecycle) {
    let current = fixture
        .syndic
        .current_binding(
            &fixture.store,
            thread(seed),
            syndic_storage::SyndicPointReadLimit::new(65536).unwrap(),
        )
        .unwrap()
        .unwrap();
    let selected = current.binding().selected_path();
    let execution = fixture
        .syndic
        .thread_execution(
            &fixture.store,
            thread(seed),
            syndic_storage::SyndicPointReadLimit::new(65536).unwrap(),
        )
        .unwrap()
        .unwrap()
        .execution()
        .clone();
    let cas_thread = beryl_model::CasThreadId::new(format!("empty-binding-{seed}")).unwrap();
    let represented = syndic_storage::CasRepresentedPrefixProof::new(
        selected.tail(),
        selected.thread_revision(),
        selected.digest(),
    );
    let usable = syndic_storage::UsableCasBinding::new(
        execution.clone(),
        cas_thread.clone(),
        represented,
        beryl_model::CasNativeTurnCount::ZERO,
        beryl_model::CasConversationToolProfile::v1([1; 32]),
        syndic_storage::CasLineageProof::native(
            syndic_storage::NativeCasLineage::Fresh,
            represented,
        )
        .unwrap(),
    );
    let state = match lifecycle {
        BindingLifecycle::Valid => BindingState::valid(usable),
        BindingLifecycle::Stale => BindingState::stale(
            syndic_storage::StaleCasBinding::new(
                execution,
                cas_thread.clone(),
                None,
                None,
                None,
                None,
                None,
                "idle source recovery",
                SyndicTimestamp::from_unix_millis(5),
            )
            .unwrap(),
        ),
        BindingLifecycle::Active => BindingState::active(syndic_storage::ActiveCasBinding::new(
            usable,
            beryl_model::SyndicExecutionSnapshotId::from_bytes([seed; 16]),
            beryl_model::SyndicTurnId::from_bytes([seed; 16]),
            InputGateRevision::new(1).unwrap(),
            SyndicTimestamp::from_unix_millis(5),
        )),
        _ => panic!("unsupported binding fixture"),
    };
    let revision = beryl_model::BindingRevision::new(2).unwrap();
    let mut batch = FixtureBatch::new();
    batch
        .put(FixtureRecord::Binding(syndic_storage::BindingRecord::new(
            thread(seed),
            revision,
            selected,
            state,
        )))
        .unwrap();
    batch
        .put(FixtureRecord::BindingHead(
            syndic_storage::BindingHeadRecord::new(
                thread(seed),
                revision,
                lifecycle,
                selected.digest(),
            ),
        ))
        .unwrap();
    batch
        .put(FixtureRecord::CasThread(
            syndic_storage::CasThreadIndexRecord::new(cas_thread.clone(), thread(seed), revision),
        ))
        .unwrap();
    batch
        .put(FixtureRecord::CasThreadBinding(
            syndic_storage::CasThreadBindingIndexRecord::new(cas_thread, thread(seed), revision),
        ))
        .unwrap();
    execute(
        &fixture.store,
        fixture
            .syndic
            .fixture_contribution(fixture.syndic.revision(&fixture.store).unwrap(), batch),
    );
}

#[test]
fn idle_usable_and_stale_bindings_preserve_current_and_reuse_without_input() {
    for lifecycle in [BindingLifecycle::Valid, BindingLifecycle::Stale] {
        let fixture = Fixture::new();
        project(&fixture, 3);
        let prior = fixture.commit(fixture.prepare(None, 4));
        set_binding(&fixture, 3, lifecycle);
        set_binding(&fixture, 4, lifecycle);
        let before = fixture.store.home_revision().unwrap();
        assert!(matches!(
            request(&fixture, Some(prior.selection), 9)
                .prepare(
                    &fixture.store,
                    &fixture.state,
                    &fixture.syndic,
                    CommandCancellation::new()
                )
                .unwrap(),
            SameWindowThreadPreparation::Current { .. }
        ));
        assert_eq!(fixture.store.home_revision().unwrap(), before);
        disqualify_submission(&fixture, 4);
        let commit = settled(&fixture, prepared(&fixture, Some(prior.selection), 9));
        assert_eq!(commit.disposition, SameWindowThreadDisposition::Reused);
        assert_eq!(commit.selection.thread_id(), thread(3));
    }
}

#[test]
fn actual_active_bindings_exclude_current_and_unclaimed_candidates() {
    let fixture = Fixture::new();
    project(&fixture, 3);
    let prior = fixture.commit(fixture.prepare(None, 4));
    set_binding(&fixture, 3, BindingLifecycle::Active);
    set_binding(&fixture, 4, BindingLifecycle::Active);
    let commit = settled(&fixture, prepared(&fixture, Some(prior.selection), 9));
    assert_eq!(commit.disposition, SameWindowThreadDisposition::Created);
    assert_eq!(commit.selection.thread_id(), thread(9));
}
