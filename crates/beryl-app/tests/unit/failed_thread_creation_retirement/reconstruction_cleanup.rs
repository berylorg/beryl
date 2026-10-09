use super::*;
use crate::composer_host::{
    ComposerHostReadTarget, ComposerHostRequestId, ComposerHostRequestKey, ComposerHostRequestKind,
    ComposerHostRequestPurpose,
};

#[test]
fn cancelled_reconstruction_requires_exact_drained_runtime_before_original_source_can_retry() {
    let fixture = support::Fixture::new("cancelled-reconstruction", 141);
    let service = service(&fixture, true);
    let saved = save(&fixture, &service);
    let prior = saved.selected();
    let seals = fixture.marker_seals();
    fail(&fixture);
    let saved = saved.retire_failed_home().ok().unwrap();
    let markers = seals.capture_failed_home(&fixture.store).unwrap();
    let mut original = source(prior);
    original.saved = Some(saved);
    let mut retirement = service
        .retire_failed_claim_cleanup(original, &markers)
        .ok()
        .unwrap();
    drop(markers);
    drop(seals);
    let (_directory, store, _) = fixture.into_store();
    let mut candidate = store.recover_same_home().unwrap();
    let storage = syndic_storage::SyndicStorage::reacquire_candidate(&candidate).unwrap();
    let state = beryl_state::BerylState::reacquire_candidate(&candidate).unwrap();
    assert!(
        retirement
            .settle_predecessor_publication(&mut candidate, &storage)
            .unwrap()
    );
    let retired = retirement.into_prior_after_proven_noncommit().ok().unwrap();
    let (mut runtime, _, retired) = retired
        .reconstruct(&mut candidate, storage.clone(), &state, None)
        .ok()
        .unwrap();
    let original_runtime = runtime.selected_identity().unwrap();
    let revision = candidate
        .recovery_access()
        .unwrap()
        .home_revision()
        .unwrap();
    let (mut retired, error) = retired
        .reconstruct(&mut candidate, storage.clone(), &state, None)
        .err()
        .unwrap();
    assert!(error.contains("already owned"));
    assert_eq!(runtime.selected_identity(), Some(original_runtime));

    let foreign = support::Fixture::new("foreign-reconstruction", 151);
    fail(&foreign);
    let (_foreign_directory, foreign_store, _) = foreign.into_store();
    let mut foreign_candidate = foreign_store.recover_same_home().unwrap();
    assert!(
        retired
            .settle_cancelled_reconstruction(&mut foreign_candidate, &storage, &state, &mut runtime)
            .is_err()
    );
    assert_eq!(runtime.selected_identity(), Some(original_runtime));

    let request = ComposerHostRequestKey::new(
        original_runtime.binding(),
        ComposerHostRequestId::new(std::num::NonZeroU64::new(999).unwrap()),
        ComposerHostRequestPurpose::Viewport,
    );
    runtime
        .test_selected_host_mut()
        .unwrap()
        .begin_request(
            request,
            ComposerHostRequestKind::Text {
                target: ComposerHostReadTarget::Candidate,
                demand: syndic_storage::DraftPieceTextDemandV1::Validate(0),
                max_bytes: 4,
            },
        )
        .unwrap();
    assert!(
        retired
            .settle_cancelled_reconstruction(&mut candidate, &storage, &state, &mut runtime)
            .is_err()
    );
    assert_eq!(runtime.selected_identity(), Some(original_runtime));
    assert!(
        runtime
            .test_selected_host_mut()
            .unwrap()
            .cancel_request(request)
    );
    retired
        .settle_cancelled_reconstruction(&mut candidate, &storage, &state, &mut runtime)
        .unwrap();
    assert!(runtime.selected_identity().is_none());
    assert_eq!(
        candidate
            .recovery_access()
            .unwrap()
            .home_revision()
            .unwrap(),
        revision
    );
    let (runtime, _, _) = retired
        .reconstruct(&mut candidate, storage.clone(), &state, None)
        .ok()
        .unwrap();
    assert_eq!(runtime.selected_identity(), Some(original_runtime));
    assert_eq!(
        runtime
            .selected_identity()
            .unwrap()
            .binding()
            .candidate()
            .session_id(),
        prior.binding().candidate().session_id()
    );
    assert_eq!(
        candidate
            .recovery_access()
            .unwrap()
            .home_revision()
            .unwrap(),
        revision
    );
}
