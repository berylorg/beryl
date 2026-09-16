use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};

use beryl_home_store::{
    HomeOpenCandidate, HomeOpenOptions, HomeSchemaVersion,
    test_faults::{FaultController, FaultPoint},
};
use beryl_state::BerylState;

use super::*;
use crate::cas_projection::{MinimumTurnCaptureReserve, ScheduledExecutionProviderContext};

#[derive(Default)]
struct Observations {
    attached: AtomicUsize,
    issued: AtomicUsize,
    shutdown: AtomicUsize,
    shutdown_health: Mutex<Option<HomeHealthState>>,
}

struct Provider {
    reference: HomeServiceReference,
    observations: Arc<Observations>,
    panic_on_attach: bool,
    shutdown_barrier: Option<(mpsc::SyncSender<()>, mpsc::Receiver<()>)>,
}

impl ScheduledOrdinaryExecutionProvider for Provider {
    fn attach(&mut self, _: ScheduledExecutionProviderContext) {
        assert_eq!(self.reference.health().state(), HomeHealthState::Opening);
        assert!(self.reference.home_revision().is_err());
        self.observations.attached.fetch_add(1, Ordering::SeqCst);
        assert!(!self.panic_on_attach, "fixture provider attachment failure");
    }

    fn try_issue(
        &mut self,
        admission: ScheduledOrdinaryAdmission,
    ) -> Result<ScheduledOrdinaryAdmissionResult, ScheduledOrdinaryAdmissionError> {
        self.observations.issued.fetch_add(1, Ordering::SeqCst);
        Ok(admission.decline(ScheduledOrdinaryExecutionUnavailable::RuntimeNotReady))
    }

    fn shutdown(&mut self) {
        *self.observations.shutdown_health.lock().unwrap() = Some(self.reference.health().state());
        self.observations.shutdown.fetch_add(1, Ordering::SeqCst);
        if let Some((arrived, release)) = self.shutdown_barrier.take() {
            arrived.send(()).unwrap();
            release.recv_timeout(Duration::from_secs(10)).unwrap();
        }
    }
}

fn candidate() -> (
    tempfile::TempDir,
    HomeOpenPublication,
    SyndicStorage,
    FaultController,
) {
    let directory = tempfile::tempdir().unwrap();
    let faults = FaultController::new();
    let mut candidate = HomeOpenCandidate::open_with_faults(
        HomeOpenOptions::new(directory.path(), HomeSchemaVersion::CURRENT),
        faults.clone(),
    )
    .unwrap();
    let storage = SyndicStorage::register(&mut candidate).unwrap();
    BerylState::register(&mut candidate).unwrap();
    let candidate = candidate
        .prepare_publication(
            BerylState::required_domains()
                .unwrap()
                .merge(SyndicStorage::required_domains().unwrap())
                .unwrap(),
        )
        .unwrap();
    (directory, candidate, storage, faults)
}

fn config() -> ProjectionServiceConfig {
    ProjectionServiceConfig::try_new(8, 4, MinimumTurnCaptureReserve::try_new(1).unwrap()).unwrap()
}

fn provider(candidate: &HomeOpenPublication, observations: &Arc<Observations>) -> Provider {
    Provider {
        reference: candidate.service_reference(),
        observations: Arc::clone(observations),
        panic_on_attach: false,
        shutdown_barrier: None,
    }
}

fn assert_reopens(directory: &tempfile::TempDir) {
    HomeOpenCandidate::open(HomeOpenOptions::new(
        directory.path(),
        HomeSchemaVersion::CURRENT,
    ))
    .unwrap();
}

#[test]
fn prepared_workers_stay_dormant_and_abandonment_joins_before_candidate_retirement() {
    let (directory, candidate, storage, _) = candidate();
    let reference = candidate.service_reference();
    let observations = Arc::new(Observations::default());
    let mut provider = provider(&candidate, &observations);
    let (arrived, waiting) = mpsc::sync_channel(1);
    let (release, proceed) = mpsc::sync_channel(1);
    provider.shutdown_barrier = Some((arrived, proceed));
    let prepared = PreparedCasServices::prepare(
        Default::default(),
        candidate,
        storage,
        config(),
        Box::new(provider),
    )
    .unwrap();
    let signal = prepared.service.as_ref().unwrap().scheduler_signal.clone();
    signal.wake(AcceptedInputWakeReason::CancellationRequested);
    thread::sleep(Duration::from_millis(50));
    let diagnostics = signal.diagnostics();
    assert!(diagnostics.recovery_handed_off());
    assert!(diagnostics.startup_recovery_page_reads() > 0);
    assert_eq!(diagnostics.pass_count(), 0);
    assert!(!diagnostics.stopped());
    assert_eq!(observations.attached.load(Ordering::SeqCst), 1);
    assert_eq!(observations.issued.load(Ordering::SeqCst), 0);
    assert_eq!(reference.health().state(), HomeHealthState::Opening);
    assert!(reference.home_revision().is_err());
    let disposer = thread::spawn(move || drop(prepared));
    waiting.recv_timeout(Duration::from_secs(10)).unwrap();
    assert!(signal.diagnostics().stopped());
    assert_eq!(reference.health().state(), HomeHealthState::Opening);
    assert!(
        HomeOpenCandidate::open(HomeOpenOptions::new(
            directory.path(),
            HomeSchemaVersion::CURRENT
        ))
        .is_err()
    );
    release.send(()).unwrap();
    disposer.join().unwrap();
    assert_eq!(
        *observations.shutdown_health.lock().unwrap(),
        Some(HomeHealthState::Opening)
    );
    assert_eq!(observations.shutdown.load(Ordering::SeqCst), 1);
    assert_reopens(&directory);
}

#[test]
fn attachment_panic_cancels_partial_service_before_candidate_retirement() {
    let (directory, candidate, storage, _) = candidate();
    let observations = Arc::new(Observations::default());
    let mut provider = provider(&candidate, &observations);
    provider.panic_on_attach = true;
    assert!(
        catch_unwind(AssertUnwindSafe(|| {
            let _ = PreparedCasServices::prepare(
                Default::default(),
                candidate,
                storage,
                config(),
                Box::new(provider),
            );
        }))
        .is_err()
    );
    assert_eq!(observations.shutdown.load(Ordering::SeqCst), 1);
    assert_eq!(
        *observations.shutdown_health.lock().unwrap(),
        Some(HomeHealthState::Opening)
    );
    assert_reopens(&directory);
}

#[test]
fn recovery_read_failure_returns_original_error_before_provider_attachment() {
    let (directory, candidate, storage, faults) = candidate();
    let observations = Arc::new(Observations::default());
    let provider = provider(&candidate, &observations);
    faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(matches!(
        PreparedCasServices::prepare(
            Default::default(),
            candidate,
            storage,
            config(),
            Box::new(provider)
        ),
        Err(CasPreparationError::Service(
            ProjectionCoordinatorError::AcceptedDeliveryRecoveryRead
        ))
    ));
    assert_eq!(observations.attached.load(Ordering::SeqCst), 0);
    assert_eq!(observations.issued.load(Ordering::SeqCst), 0);
    assert_reopens(&directory);
}

#[test]
fn foreign_storage_is_rejected_before_provider_attachment() {
    let (directory, candidate, _, _) = candidate();
    let (_foreign_directory, foreign, storage, _) = self::candidate();
    let observations = Arc::new(Observations::default());
    let provider = provider(&candidate, &observations);
    assert!(matches!(
        PreparedCasServices::prepare(
            Default::default(),
            candidate,
            storage,
            config(),
            Box::new(provider)
        ),
        Err(CasPreparationError::Service(
            ProjectionCoordinatorError::AcceptedDeliveryRecoveryRead
        ))
    ));
    assert_eq!(observations.attached.load(Ordering::SeqCst), 0);
    assert_eq!(foreign.health().state(), HomeHealthState::Opening);
    assert_reopens(&directory);
}

#[test]
fn test_publication_releases_prepared_workers_in_the_same_generation() {
    let (_directory, candidate, storage, _) = candidate();
    let generation = candidate.generation();
    let observations = Arc::new(Observations::default());
    let provider = provider(&candidate, &observations);
    let mut prepared = PreparedCasServices::prepare(
        Default::default(),
        candidate,
        storage,
        config(),
        Box::new(provider),
    )
    .unwrap();
    let mut service = prepared.service.take().unwrap();
    let home = prepared.candidate.take().unwrap().publish().unwrap();
    assert_eq!(home.health().generation(), Some(generation));
    assert_eq!(service.home_generation, generation);
    service.owned_home = Some(home);
    assert!(prepared.initial_start.take().unwrap().release());
    let deadline = Instant::now() + Duration::from_secs(10);
    while service.scheduler_signal.diagnostics().pass_count() == 0 {
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(5));
    }
    let _ = service.close().unwrap();
    assert_eq!(observations.shutdown.load(Ordering::SeqCst), 1);
}
