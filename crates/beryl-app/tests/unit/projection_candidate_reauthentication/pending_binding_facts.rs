enum Phase83PendingBindingFact {
    Execution(ExecutionBinding),
    CasThread(CasThreadId),
    Lineage(CasLineageProof),
    SelectedPath(SelectedPathProof),
}

fn phase83_replace_pending_binding_fact(
    fixture: &Phase83Fixture,
    replacement: Phase83PendingBindingFact,
) {
    let current = fixture
        .storage
        .current_binding(
            &fixture.home,
            fixture.syndic_thread_id,
            phase83_point_limit(),
        )
        .unwrap()
        .unwrap();
    let binding = current.binding();
    let syndic_storage::BindingState::Valid(usable) = binding.state() else {
        panic!("the Phase 83 pending fixture retains one valid binding")
    };
    let mut execution = usable.execution().clone();
    let mut cas_thread_id = usable.cas_thread_id().clone();
    let mut lineage = usable.lineage();
    let mut selected_path = binding.selected_path();
    match replacement {
        Phase83PendingBindingFact::Execution(value) => execution = value,
        Phase83PendingBindingFact::CasThread(value) => cas_thread_id = value,
        Phase83PendingBindingFact::Lineage(value) => lineage = value,
        Phase83PendingBindingFact::SelectedPath(value) => selected_path = value,
    }
    let changed = syndic_storage::BindingRecord::new(
        binding.thread_id(),
        binding.revision(),
        selected_path,
        syndic_storage::BindingState::valid(syndic_storage::UsableCasBinding::new(
            execution,
            cas_thread_id,
            usable.represented_prefix(),
            usable.native_turn_count(),
            usable.tool_profile(),
            lineage,
        )),
    );
    let mut batch = FixtureBatch::new();
    batch.put(FixtureRecord::Binding(changed)).unwrap();
    batch
        .put(FixtureRecord::BindingHead(
            syndic_storage::BindingHeadRecord::new(
                current.head().thread_id(),
                current.head().revision(),
                current.head().lifecycle(),
                selected_path.digest(),
            ),
        ))
        .unwrap();
    fixture.apply_fixture_batch(batch);
}

fn phase83_replace_pending_execution_binding(
    fixture: &Phase83Fixture,
    execution: ExecutionBinding,
) {
    phase83_replace_pending_binding_fact(
        fixture,
        Phase83PendingBindingFact::Execution(execution),
    );
}

fn phase83_replace_pending_cas_thread(fixture: &Phase83Fixture, cas_thread_id: CasThreadId) {
    phase83_replace_pending_binding_fact(
        fixture,
        Phase83PendingBindingFact::CasThread(cas_thread_id),
    );
}

fn phase83_replace_pending_lineage(fixture: &Phase83Fixture, lineage: CasLineageProof) {
    phase83_replace_pending_binding_fact(fixture, Phase83PendingBindingFact::Lineage(lineage));
}

fn phase83_replace_pending_selected_path(
    fixture: &Phase83Fixture,
    selected_path: SelectedPathProof,
) {
    phase83_replace_pending_binding_fact(
        fixture,
        Phase83PendingBindingFact::SelectedPath(selected_path),
    );
}

#[test]
fn phase83_pending_binding_revision_mismatch_is_retryable_and_owning() {
    phase83_assert_retryable_fact_rejection(
        Phase83Fixture::new(243, 1, false),
        ProjectionCandidateReauthenticationReason::PendingOrdinaryProjectionMismatch,
    );
}

#[test]
fn phase83_pending_execution_binding_mismatch_is_retryable_and_owning() {
    let fixture = Phase83Fixture::new(244, 1, true);
    let replacement = phase79_execution_binding(RuntimeId::from_bytes([0; 16]), 244);
    phase83_replace_pending_execution_binding(&fixture, replacement);

    phase83_assert_retryable_fact_rejection(
        fixture,
        ProjectionCandidateReauthenticationReason::PendingOrdinaryProjectionMismatch,
    );
}

#[test]
fn phase83_pending_cas_thread_mismatch_is_retryable_and_owning() {
    let fixture = Phase83Fixture::new(245, 1, true);
    phase83_replace_pending_cas_thread(
        &fixture,
        CasThreadId::new("phase-83-pending-binding-cas-thread-mismatch").unwrap(),
    );

    phase83_assert_retryable_fact_rejection(
        fixture,
        ProjectionCandidateReauthenticationReason::PendingOrdinaryProjectionMismatch,
    );
}

#[test]
fn phase83_pending_lineage_mismatch_is_retryable_and_owning() {
    let fixture = Phase83Fixture::new(246, 1, true);
    let current = fixture
        .storage
        .current_binding(
            &fixture.home,
            fixture.syndic_thread_id,
            phase83_point_limit(),
        )
        .unwrap()
        .unwrap();
    let syndic_storage::BindingState::Valid(usable) = current.binding().state() else {
        panic!("the Phase 83 pending fixture retains one valid binding")
    };
    let current_lineage = usable.lineage();
    let current_prefix = current_lineage.established_prefix();
    assert_eq!(current_prefix.tail(), None);
    let replacement = CasLineageProof::native(
        NativeCasLineage::Fresh,
        CasRepresentedPrefixProof::new(
            None,
            ThreadRevision::new(current_prefix.source_thread_revision().get() + 1).unwrap(),
            current_prefix.digest(),
        ),
    )
    .unwrap();
    assert_ne!(replacement, current_lineage);
    phase83_replace_pending_lineage(&fixture, replacement);

    phase83_assert_retryable_fact_rejection(
        fixture,
        ProjectionCandidateReauthenticationReason::PendingOrdinaryProjectionMismatch,
    );
}

#[test]
fn phase83_pending_selected_path_mismatch_is_retryable_and_owning() {
    let fixture = Phase83Fixture::new(247, 1, true);
    let current = fixture
        .storage
        .current_binding(
            &fixture.home,
            fixture.syndic_thread_id,
            phase83_point_limit(),
        )
        .unwrap()
        .unwrap()
        .binding()
        .selected_path();
    let replacement = SelectedPathProof::new(
        current.tail(),
        current.thread_revision(),
        beryl_model::SyndicPathDigest::from_bytes([247; 32]),
    );
    assert_ne!(replacement, current);
    phase83_replace_pending_selected_path(&fixture, replacement);

    phase83_assert_retryable_fact_rejection(
        fixture,
        ProjectionCandidateReauthenticationReason::PendingOrdinaryProjectionMismatch,
    );
}
