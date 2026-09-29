use super::*;
use crate::{
    app_services::recovery_composer::PreparedComposerRecoveryAdapters as Adapters,
    composer_marker_seal::DraftMarkerSealServiceLimits,
};
use std::num::NonZeroUsize;

fn limits() -> DraftMarkerSealServiceLimits {
    DraftMarkerSealServiceLimits::new(NonZeroUsize::new(2).unwrap(), NonZeroUsize::new(1).unwrap())
        .unwrap()
}

fn prepare_cas(
    candidate: HomeRecoveryCandidate,
    storage: SyndicStorage,
    probe: &Arc<Probe>,
) -> (
    HomeRecoveryCandidate,
    ProjectionConnectionService,
    InitialStartOwner,
) {
    let provider = provider(&candidate, probe);
    match PreparedRecoveryCasServices::prepare(
        Default::default(),
        candidate,
        storage,
        config(),
        Box::new(provider),
        &ProjectionCancellationToken::new(),
    ) {
        Ok(prepared) => prepared.into_recovery_parts(),
        Err(_) => panic!("fixture CAS preparation failed"),
    }
}

#[test]
fn composer_adapter_preparation_and_transfer_keep_candidate_and_workers_fenced() {
    for transfer in [false, true] {
        let (directory, candidate, storage, _, _) = fixture();
        let assets = BerylState::reacquire_candidate(&candidate)
            .unwrap()
            .assets();
        let probe = Arc::new(Probe::default());
        let (mut candidate, cas, start) = prepare_cas(candidate, storage.clone(), &probe);
        let revision = assets
            .revision_candidate(&candidate.recovery_access().unwrap())
            .unwrap();
        let adapters = Adapters::prepare(
            &mut candidate,
            storage,
            assets,
            &cas,
            limits(),
            config().turn_start_admission_requirement(),
        )
        .unwrap();
        assert!(adapters.matches(candidate.home_id(), candidate.generation()));
        assert_eq!(
            candidate.service_reference().health().state(),
            HomeHealthState::Reopening
        );
        assert_eq!(cas.scheduler_signal.diagnostics().pass_count(), 0);
        assert_eq!(probe.issued.load(Ordering::SeqCst), 0);
        if transfer {
            let (assets, marker, submission, native) = adapters.into_parts();
            assert_eq!(
                assets
                    .revision_candidate(&candidate.recovery_access().unwrap())
                    .unwrap(),
                revision
            );
            assert_eq!(marker.diagnostics().current_flights(), 0);
            marker.retire_home_generation();
            drop((assets, marker, submission, native));
        } else {
            drop(adapters);
        }
        assert_eq!(probe.shutdown.load(Ordering::SeqCst), 0);
        let gate = start.gate();
        drop(start);
        assert!(!gate.wait());
        cas.close().unwrap();
        assert_eq!(probe.shutdown.load(Ordering::SeqCst), 1);
        candidate.abort().close().unwrap();
        HomeOpenCandidate::open(HomeOpenOptions::new(
            directory.path(),
            HomeSchemaVersion::CURRENT,
        ))
        .unwrap()
        .close()
        .unwrap();
    }
}

#[test]
fn composer_adapters_refuse_foreign_or_stale_cas_without_consuming_candidate() {
    for stale in [false, true] {
        let (_directory, candidate, storage, _, _) = fixture();
        let probe = Arc::new(Probe::default());
        let (candidate, cas, start) = prepare_cas(candidate, storage, &probe);
        drop(start);
        let (other_directory, other, _, _, _) = fixture();
        let (mut target, retained) = if stale {
            (candidate.abort().recover_same_home().unwrap(), other)
        } else {
            (other, candidate)
        };
        let storage = SyndicStorage::reacquire_candidate(&target).unwrap();
        let assets = BerylState::reacquire_candidate(&target).unwrap().assets();
        let result = Adapters::prepare(
            &mut target,
            storage.clone(),
            assets,
            &cas,
            limits(),
            config().turn_start_admission_requirement(),
        );
        assert!(result.err().unwrap().contains("another candidate"));
        assert!(
            storage
                .revision_candidate(&target.recovery_access().unwrap())
                .is_ok()
        );
        assert_eq!(probe.issued.load(Ordering::SeqCst), 0);
        cas.close().unwrap();
        target.abort().close().unwrap();
        retained.abort().close().unwrap();
        drop(other_directory);
    }
}

#[test]
fn composer_adapters_refuse_stale_storage_and_foreign_assets_without_publication() {
    for wrong_assets in [false, true] {
        let (_directory, candidate, storage, stale, _) = fixture();
        let (_other_directory, other, _, _, _) = fixture();
        let assets =
            BerylState::reacquire_candidate(if wrong_assets { &other } else { &candidate })
                .unwrap()
                .assets();
        let probe = Arc::new(Probe::default());
        let (mut candidate, cas, start) = prepare_cas(candidate, storage.clone(), &probe);
        assert!(
            Adapters::prepare(
                &mut candidate,
                if wrong_assets { storage.clone() } else { stale },
                assets,
                &cas,
                limits(),
                config().turn_start_admission_requirement(),
            )
            .is_err()
        );
        assert_eq!(
            candidate.service_reference().health().state(),
            HomeHealthState::Reopening
        );
        assert!(
            storage
                .revision_candidate(&candidate.recovery_access().unwrap())
                .is_ok()
        );
        assert_eq!(cas.scheduler_signal.diagnostics().pass_count(), 0);
        drop(start);
        cas.close().unwrap();
        candidate.abort().close().unwrap();
        other.abort().close().unwrap();
    }
}
