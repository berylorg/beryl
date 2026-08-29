use super::*;

#[test]
fn reopen_requires_creation_time_unbound_binding_revision() {
    let thread = id(55);
    let draft = draft_id(56);
    let selected = SelectedPathProof::new(
        None,
        beryl_model::ThreadRevision::new(1).unwrap(),
        empty_selected_path_digest(),
    );
    let represented =
        CasRepresentedPrefixProof::new(None, selected.thread_revision(), selected.digest());
    let cas_thread = CasThreadId::new("corrupt-creation-binding").unwrap();
    let revision = BindingRevision::new(1).unwrap();
    exercise_case(
        "phase9-creation-binding-state",
        "initial binding is not the creation-time unbound revision",
        &[(thread, draft)],
        || batch([FixtureRecord::ThreadUsage(ThreadUsageRecord::empty(thread))]),
        || {
            batch([
                FixtureRecord::Binding(BindingRecord::new(
                    thread,
                    revision,
                    selected,
                    BindingState::valid(UsableCasBinding::new(
                        execution_binding(),
                        cas_thread.clone(),
                        represented,
                        CasNativeTurnCount::ZERO,
                        test_tool_profile(),
                        CasLineageProof::native(NativeCasLineage::Fresh, represented).unwrap(),
                    )),
                )),
                FixtureRecord::BindingHead(BindingHeadRecord::new(
                    thread,
                    revision,
                    BindingLifecycle::Valid,
                    selected.digest(),
                )),
                FixtureRecord::CasThread(CasThreadIndexRecord::new(
                    cas_thread.clone(),
                    thread,
                    revision,
                )),
                FixtureRecord::CasThreadBinding(CasThreadBindingIndexRecord::new(
                    cas_thread.clone(),
                    thread,
                    revision,
                )),
            ])
        },
    );
}

#[test]
fn reopen_rejects_persisted_binding_that_claims_the_pending_tail() {
    let home = TestHome::new("phase9-persisted-pending-claim");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let (thread, None, turn, selected) = fault_pending_path(&store, &storage, 57, false) else {
        unreachable!()
    };
    let represented =
        CasRepresentedPrefixProof::new(Some(turn), selected.thread_revision(), selected.digest());
    let cas_thread = CasThreadId::new("corrupt-pending-claim").unwrap();
    let revision = current_binding_revision(&store, &storage, thread)
        .checked_next()
        .unwrap();
    let usable = UsableCasBinding::new(
        storage
            .thread_execution(&store, thread, point_limit())
            .unwrap()
            .unwrap()
            .execution()
            .clone(),
        cas_thread.clone(),
        represented,
        CasNativeTurnCount::ZERO,
        test_tool_profile(),
        CasLineageProof::native(NativeCasLineage::Continuation, represented).unwrap(),
    );
    commit(
        &store,
        storage,
        batch([
            FixtureRecord::Binding(BindingRecord::new(
                thread,
                revision,
                selected,
                BindingState::valid(usable),
            )),
            FixtureRecord::BindingHead(BindingHeadRecord::new(
                thread,
                revision,
                BindingLifecycle::Valid,
                selected.digest(),
            )),
            FixtureRecord::CasThread(CasThreadIndexRecord::new(
                cas_thread.clone(),
                thread,
                revision,
            )),
            FixtureRecord::CasThreadBinding(CasThreadBindingIndexRecord::new(
                cas_thread, thread, revision,
            )),
        ]),
    );
    store.close().unwrap();

    let mut reopened = open(home.path());
    let error = match SyndicStorage::register_with_schema_validation(&mut reopened) {
        Ok(_) => panic!("persisted pending-tail claim reopened successfully"),
        Err(error) => error,
    };
    match error {
        beryl_home_store::DomainRegistrationError::Validation { domain, source } => {
            assert_eq!(domain, "syndic");
            assert_eq!(
                source.to_string(),
                "pending binding does not represent exactly its parent prefix"
            );
        }
        other => panic!("expected pending-tail claim rejection, got {other:?}"),
    }
    reopened.close().unwrap();
}

#[test]
fn reopen_rejects_first_cas_membership_established_at_another_prefix() {
    let home = TestHome::new("phase9-first-membership-establishment");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let (thread, Some(parent), _, selected) = fault_pending_path(&store, &storage, 60, true) else {
        unreachable!()
    };
    let parent = storage
        .turn(&store, parent, point_limit())
        .unwrap()
        .unwrap();
    let represented = CasRepresentedPrefixProof::new(
        Some(parent.id()),
        selected.thread_revision(),
        parent.chain_digest(),
    );
    let established = CasRepresentedPrefixProof::new(
        None,
        selected.thread_revision(),
        empty_selected_path_digest(),
    );
    let cas_thread = CasThreadId::new("corrupt-first-establishment").unwrap();
    let revision = current_binding_revision(&store, &storage, thread)
        .checked_next()
        .unwrap();
    let usable = UsableCasBinding::new(
        storage
            .thread_execution(&store, thread, point_limit())
            .unwrap()
            .unwrap()
            .execution()
            .clone(),
        cas_thread.clone(),
        represented,
        CasNativeTurnCount::ZERO,
        test_tool_profile(),
        CasLineageProof::native(NativeCasLineage::Fresh, established).unwrap(),
    );
    commit(
        &store,
        storage,
        batch([
            FixtureRecord::Binding(BindingRecord::new(
                thread,
                revision,
                selected,
                BindingState::valid(usable),
            )),
            FixtureRecord::BindingHead(BindingHeadRecord::new(
                thread,
                revision,
                BindingLifecycle::Valid,
                selected.digest(),
            )),
            FixtureRecord::CasThread(CasThreadIndexRecord::new(
                cas_thread.clone(),
                thread,
                revision,
            )),
            FixtureRecord::CasThreadBinding(CasThreadBindingIndexRecord::new(
                cas_thread, thread, revision,
            )),
        ]),
    );
    store.close().unwrap();

    let mut reopened = open(home.path());
    let error = match SyndicStorage::register_with_schema_validation(&mut reopened) {
        Ok(_) => panic!("mismatched first CAS establishment reopened successfully"),
        Err(error) => error,
    };
    match error {
        beryl_home_store::DomainRegistrationError::Validation { domain, source } => {
            assert_eq!(domain, "syndic");
            assert_eq!(
                source.to_string(),
                "first usable CAS membership was not established at its represented prefix"
            );
        }
        other => panic!("expected first-establishment rejection, got {other:?}"),
    }
    reopened.close().unwrap();
}
