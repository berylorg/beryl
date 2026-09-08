use super::*;
use syndic_storage::{
    DraftMarkerAdmissionLimitsV1, DraftMarkerAdmissionOperationIdV1, DraftMarkerAdmissionOwnerV1,
    DraftMutationOperationIdV1, DraftMutationStagingIdentityV1, DraftMutationStagingStatusV1,
};

pub(super) fn identity(
    binding: beryl_app::composer_host::ComposerHostBinding,
    operation: u64,
) -> DraftMutationStagingIdentityV1 {
    let mut bytes = [0; 16];
    bytes[8..].copy_from_slice(&operation.to_be_bytes());
    DraftMutationStagingIdentityV1::new(
        binding.candidate().draft_id(),
        binding.candidate().session_id(),
        DraftMutationOperationIdV1::from_bytes(bytes),
    )
}

fn owner(
    binding: beryl_app::composer_host::ComposerHostBinding,
    operation: u64,
) -> DraftMarkerAdmissionOwnerV1 {
    let identity = identity(binding, operation);
    DraftMarkerAdmissionOwnerV1::new(
        identity.draft_id(),
        identity.session_id(),
        DraftMarkerAdmissionOperationIdV1::from_bytes(*identity.operation_id().as_bytes()),
    )
}

fn assert_no_durable_begin(
    fixture: &support::Fixture,
    binding: beryl_app::composer_host::ComposerHostBinding,
    operation: u64,
) {
    assert_eq!(
        fixture
            .storage
            .draft_mutation_staging_status(&fixture.store, identity(binding, operation))
            .unwrap(),
        DraftMutationStagingStatusV1::Absent
    );
}

#[test]
fn evidence_size_and_shared_capacity_refusals_are_distinct_and_release_exact_custody() {
    for (seed, limits, too_large) in [
        (60, DraftMarkerAdmissionLimitsV1::new(64, 0, u64::MAX), true),
        (
            70,
            DraftMarkerAdmissionLimitsV1::new(0, u64::MAX, u64::MAX),
            false,
        ),
    ] {
        let fixture = fixture("composer-marker-admission-refusal", seed);
        let asset = publish_image_asset(&fixture.store, fixture.assets.clone(), b"limited-image");
        let (mut host, binding) = activate(
            fixture.storage.clone(),
            &fixture.store,
            fixture.thread,
            seed + 1,
            seed + 2,
        );
        host.test_set_mutation_admission_retained_limits(limits);
        let operation = u64::from(seed + 3);
        let (mut editor, begin, key) = begin_edit(
            binding,
            operation,
            SourceRange::new(origin(), origin()).unwrap(),
            operation + 1000,
        );
        let pass = editor.request_evidence(key).unwrap();
        assert!(
            matches!(drive_evidence(&mut host, &fixture.store, binding, &fixture.assets,
            ComposerHostMutationEvidenceRequest::Begin { begin, pass }),
            ComposerHostMutationEvidenceOutcome::Started(actual) if actual == pass)
        );
        assert_eq!(host.settlement_custody_in_use(), 1);
        assert_no_durable_begin(&fixture, binding, operation);
        let object = InlineObjectId::new(601);
        let proposal = page(
            &editor,
            key,
            MutationLane::Proposal,
            vec![MutationPageItem::Object(ObjectChange::Insert {
                object: SuccessorObject::new(
                    object,
                    ByteOffset::new(0),
                    InlineObjectOrder::new(1),
                    1,
                    1,
                ),
            })],
        );
        let outcome = drive_evidence(
            &mut host,
            &fixture.store,
            binding,
            &fixture.assets,
            ComposerHostMutationEvidenceRequest::Page {
                pass,
                page: proposal,
                metadata: Box::new([ComposerHostImageMarkerMetadata::new(object, asset)]),
            },
        );
        let ComposerHostMutationEvidenceOutcome::Refused {
            key: actual,
            failure,
        } = outcome
        else {
            panic!("real admission limit did not produce a determinate refusal");
        };
        assert_eq!(actual, key);
        if too_large {
            assert!(matches!(
                failure.as_ref(),
                ComposerHostMutationAdmissionFailure::OperationTooLarge
            ));
        } else {
            assert!(matches!(
                failure.as_ref(),
                ComposerHostMutationAdmissionFailure::CapacityUnavailable
            ));
        }
        assert_eq!(host.binding(), Some(binding));
        assert_eq!(host.settlement_custody_in_use(), 0);
        assert_no_durable_begin(&fixture, binding, operation);
        let snapshot = fixture
            .storage
            .draft_marker_admission_publication_snapshot_for_test(
                &fixture.store,
                owner(binding, operation),
                &[],
            )
            .unwrap();
        assert!(snapshot.head().is_none());
        assert!(
            snapshot
                .capacity()
                .is_none_or(|capacity| capacity.charge().associations() == 0)
        );

        host.test_set_mutation_admission_retained_limits(DraftMarkerAdmissionLimitsV1::new(
            64,
            u64::MAX,
            u64::MAX,
        ));
        let (mut next, begin, key) = begin_edit(
            binding,
            operation + 1,
            SourceRange::new(origin(), origin()).unwrap(),
            operation + 1001,
        );
        let pass = next.request_evidence(key).unwrap();
        assert!(
            matches!(drive_evidence(&mut host, &fixture.store, binding, &fixture.assets,
            ComposerHostMutationEvidenceRequest::Begin { begin, pass }),
            ComposerHostMutationEvidenceOutcome::Started(actual) if actual == pass)
        );
        assert!(
            matches!(cancel_evidence(&mut host, &fixture.store, binding, &fixture.assets, key),
            ComposerHostMutationEvidenceOutcome::Refused { failure, .. }
                if matches!(failure.as_ref(), ComposerHostMutationAdmissionFailure::Cancelled))
        );
        assert_eq!(host.settlement_custody_in_use(), 0);
    }
}

#[test]
fn evidence_storage_noncommit_preserves_its_failure_and_the_exact_prior_candidate() {
    let fixture = fixture("composer-marker-storage-refusal", 80);
    let asset = publish_image_asset(&fixture.store, fixture.assets.clone(), b"fault-image");
    let (mut host, binding) = activate(
        fixture.storage.clone(),
        &fixture.store,
        fixture.thread,
        81,
        82,
    );
    let (mut editor, begin, key) = begin_edit(
        binding,
        83,
        SourceRange::new(origin(), origin()).unwrap(),
        1083,
    );
    let pass = editor.request_evidence(key).unwrap();
    assert!(
        matches!(drive_evidence(&mut host, &fixture.store, binding, &fixture.assets,
        ComposerHostMutationEvidenceRequest::Begin { begin, pass }),
        ComposerHostMutationEvidenceOutcome::Started(actual) if actual == pass)
    );
    let revision = fixture.store.home_revision().unwrap();
    let object = InlineObjectId::new(801);
    let proposal = page(
        &editor,
        key,
        MutationLane::Proposal,
        vec![MutationPageItem::Object(ObjectChange::Insert {
            object: SuccessorObject::new(
                object,
                ByteOffset::new(0),
                InlineObjectOrder::new(1),
                1,
                1,
            ),
        })],
    );
    fixture
        .faults
        .fail_next(beryl_home_store::test_faults::FaultPoint::BeforeCommit);
    let outcome = drive_evidence(
        &mut host,
        &fixture.store,
        binding,
        &fixture.assets,
        ComposerHostMutationEvidenceRequest::Page {
            pass,
            page: proposal,
            metadata: Box::new([ComposerHostImageMarkerMetadata::new(object, asset)]),
        },
    );
    assert!(matches!(outcome,
    ComposerHostMutationEvidenceOutcome::Refused { key: actual, failure }
        if actual == key && matches!(failure.as_ref(), ComposerHostMutationAdmissionFailure::Storage(
            syndic_storage::DraftMarkerAdmissionStorageErrorV1::Command(beryl_home_store::CommandError::Commit { .. })
        ))));
    assert_eq!(host.binding(), Some(binding));
    assert_eq!(host.settlement_custody_in_use(), 0);
    let recovery = fixture.store.recover_same_home().unwrap();
    let storage = SyndicStorage::reacquire_candidate(&recovery).unwrap();
    let store = recovery.publish();
    assert_eq!(store.home_revision().unwrap(), revision);
    assert_eq!(
        storage
            .draft_mutation_staging_status(&store, identity(binding, 83))
            .unwrap(),
        DraftMutationStagingStatusV1::Absent
    );
    assert!(
        storage
            .draft_marker_admission_publication_snapshot_for_test(&store, owner(binding, 83), &[])
            .unwrap()
            .head()
            .is_none()
    );
    let syndic_storage::DraftEditorCandidateSessionReadOutcomeV1::Active(session) = storage
        .draft_editor_candidate_session(
            &store,
            binding.candidate().draft_id(),
            binding.candidate().session_id(),
        )
        .unwrap()
    else {
        panic!("storage refusal changed candidate liveness");
    };
    assert_eq!(
        syndic_storage::DraftEditorCandidateActivationBindingV1::from_head(&session),
        binding.candidate()
    );
}

#[test]
fn stale_operation_and_mismatched_predecessor_generation_never_enter_evidence_admission() {
    let fixture = fixture("composer-marker-stale-evidence", 90);
    let (mut host, binding) = activate(
        fixture.storage.clone(),
        &fixture.store,
        fixture.thread,
        91,
        92,
    );
    let (mut editor, begin, key) = begin_edit(
        binding,
        94,
        SourceRange::new(origin(), origin()).unwrap(),
        1094,
    );
    let pass = editor.request_evidence(key).unwrap();
    assert!(
        matches!(drive_evidence(&mut host, &fixture.store, binding, &fixture.assets,
        ComposerHostMutationEvidenceRequest::Begin { begin, pass }),
        ComposerHostMutationEvidenceOutcome::Started(actual) if actual == pass)
    );
    assert!(matches!(
        cancel_evidence(&mut host, &fixture.store, binding, &fixture.assets, key),
        ComposerHostMutationEvidenceOutcome::Refused { .. }
    ));
    let revision = fixture.store.home_revision().unwrap();
    let (mut old, begin, key) = begin_edit(
        binding,
        93,
        SourceRange::new(origin(), origin()).unwrap(),
        1093,
    );
    let pass = old.request_evidence(key).unwrap();
    assert!(matches!(
        host.dispatch_mutation_evidence(
            &fixture.store,
            binding,
            &fixture.assets,
            ComposerHostMutationEvidenceRequest::Begin { begin, pass },
            &CommandCancellation::new()
        ),
        Err(ComposerHostError::StaleRequestIdentity)
    ));

    let (mut editor, begin, key) = begin_edit(
        binding,
        95,
        SourceRange::new(origin(), origin()).unwrap(),
        1095,
    );
    let pass = editor.request_evidence(key).unwrap();
    for (key, predecessor) in [
        (
            MutationKey::new(
                gpui_text_input::BindingId::new(key.binding().get() + 1),
                key.base_revision(),
                key.operation(),
            ),
            MutationPositions::collapsed(origin()),
        ),
        (
            MutationKey::new(
                key.binding(),
                SourceRevision::new(key.base_revision().get() + 1),
                key.operation(),
            ),
            MutationPositions::collapsed(origin()),
        ),
        (
            key,
            MutationPositions::new(
                origin(),
                origin(),
                SourcePosition::new(ByteOffset::new(1), InlineObjectGap::NoObjects),
            ),
        ),
    ] {
        let invalid = MutationBeginRequest::new(
            MutationProposal::new(
                key,
                MutationKind::Edit,
                predecessor,
                SourceRange::new(origin(), origin()).unwrap(),
                0,
            ),
            begin.source_cursor(),
            begin.proposal_cursor(),
        )
        .with_replayable_producer(pass.producer());
        assert!(matches!(
            host.dispatch_mutation_evidence(
                &fixture.store,
                binding,
                &fixture.assets,
                ComposerHostMutationEvidenceRequest::Begin {
                    begin: invalid,
                    pass
                },
                &CommandCancellation::new()
            ),
            Err(ComposerHostError::MutationMalformed)
        ));
        assert_eq!(host.binding(), Some(binding));
        assert_eq!(host.settlement_custody_in_use(), 0);
        assert_eq!(fixture.store.home_revision().unwrap(), revision);
    }
    assert_no_durable_begin(&fixture, binding, 93);
    assert_no_durable_begin(&fixture, binding, 95);
}
