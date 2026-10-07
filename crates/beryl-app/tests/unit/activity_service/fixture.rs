use super::*;
use crate::{
    cas_projection::{
        RuntimeInterest, RuntimeInterestConfig, RuntimeInterestError, RuntimeInterestKind,
        RuntimeInterestStatus, RuntimeInterestTestHarness, RuntimeInterestTestProbe,
    },
    runtime_activity_enrollment::RuntimeActivityEnrollmentOperations,
};
use beryl_backend::ManagedBackendLaunchSpec;
use beryl_home_store::{
    CommandOutcome, HomeCommand, HomeOpenCandidate, HomeOpenOptions, HomeSchemaVersion,
    test_faults::FaultController,
};
use beryl_model::{
    AdmittedHostPath, CasItemId, CasProcessGeneration, ExecutionBinding, PathFlavor, RuntimeMode,
    RuntimeNativePath, SyndicItemId, SyndicTurnId,
};
use std::time::Duration;
use support::{TestHome, draft_id, timestamp};
use syndic_storage::*;

pub(super) const TIMEOUT: Duration = Duration::from_secs(10);

pub(super) fn limits(collections: usize, pages: usize, rows: usize) -> ActivityServiceLimits {
    ActivityServiceLimits::new(
        NonZeroUsize::new(collections).unwrap(),
        NonZeroUsize::new(pages).unwrap(),
        CursorReadLimits::new(rows, 65_536).unwrap(),
    )
}

pub(super) fn owner(home_id: BerylHomeId) -> RuntimeInterestTestHarness {
    RuntimeInterestTestHarness::with_enrollments(
        RuntimeInterestConfig::new(
            NonZeroUsize::new(1).unwrap(),
            NonZeroUsize::new(4).unwrap(),
            TIMEOUT,
        )
        .unwrap(),
        RuntimeActivityEnrollmentOperations::new(home_id, NonZeroUsize::new(1).unwrap()),
    )
}

pub(super) struct Fixture {
    pub home: HomeStore,
    pub storage: SyndicStorage,
    pub faults: FaultController,
    pub binding: ExecutionBinding,
    pub turn: SyndicTurnId,
    pub interest: Option<Arc<RuntimeInterest>>,
    pub owner: RuntimeInterestTestHarness,
    pub probe: RuntimeInterestTestProbe,
    _directory: TestHome,
}

impl Fixture {
    pub fn new(enroll: bool) -> Self {
        let directory = TestHome::new("activity-service");
        let faults = FaultController::new();
        let mut candidate = HomeOpenCandidate::open_with_faults(
            HomeOpenOptions::new(directory.path(), HomeSchemaVersion::CURRENT),
            faults.clone(),
        )
        .unwrap();
        let storage = SyndicStorage::register(&mut candidate).unwrap();
        let home = candidate
            .prepare_publication(SyndicStorage::required_domains().unwrap())
            .unwrap()
            .publish()
            .unwrap();
        support::seed_populated(&home, storage.clone());
        support::converge_and_release_terminal_history(
            &home,
            storage.clone(),
            id(30),
            support::populated::source_turn(),
        );
        let turn = support::exact_cas::submit_current_draft(
            &home,
            storage.clone(),
            id(30),
            draft_id(220),
            SyndicItemId::from_bytes([221; 16]),
            "next",
            timestamp(100),
        );
        let binding = storage
            .thread_execution(&home, id(30), point_limit())
            .unwrap()
            .unwrap()
            .execution()
            .clone();
        let owner = owner(home.home_id());
        let probe = RuntimeInterestTestProbe::new(CasProcessGeneration::new(91).unwrap());
        let mut result = Self {
            home,
            storage,
            faults,
            binding,
            turn,
            interest: None,
            owner,
            probe,
            _directory: directory,
        };
        result.acquire(enroll);
        result
    }

    pub fn acquire(&mut self, enroll: bool) {
        self.probe = RuntimeInterestTestProbe::new(CasProcessGeneration::new(91).unwrap());
        let native = |path| {
            RuntimeNativePath::from_admitted(RuntimeMode::Host, PathFlavor::Windows, path).unwrap()
        };
        let spec = ManagedBackendLaunchSpec::new(
            self.binding.runtime_id(),
            AdmittedHostPath::from_admitted(PathFlavor::Windows, r"C:\activity-test\codex.exe")
                .unwrap(),
            RuntimeMode::Host,
            beryl_model::RuntimeLaunchForm::CodexCli,
            native(r"C:\activity-test\codex.exe"),
            self.binding.root_path().clone(),
            AdmittedHostPath::from_admitted(PathFlavor::Windows, r"C:\activity-test\tokens")
                .unwrap(),
            native(r"C:\activity-test\tokens"),
        )
        .unwrap();
        let deadline = std::time::Instant::now() + TIMEOUT;
        let interest = loop {
            match self.owner.acquire(
                spec.clone(),
                self.binding.clone(),
                RuntimeInterestKind::RequiredWork,
                self.probe.clone(),
            ) {
                Ok(interest) => break interest,
                Err(RuntimeInterestError::Retiring) if std::time::Instant::now() < deadline => {
                    std::thread::yield_now();
                }
                Err(error) => panic!("runtime fixture admission: {error:?}"),
            }
        };
        assert!(matches!(
            interest.wait_for_change(RuntimeInterestStatus::Starting, TIMEOUT),
            RuntimeInterestStatus::Ready(_)
        ));
        if enroll {
            interest
                .enroll_activity_for_test(&self.home, &self.storage, id(30), self.turn)
                .unwrap();
        }
        self.interest = Some(Arc::new(interest));
    }

    pub fn service(&self, limits: ActivityServiceLimits) -> ActivityService {
        let runtime = self
            .owner
            .activity_read_source(
                self.home.home_id(),
                self.home.health().generation().unwrap(),
            )
            .unwrap();
        let service = ActivityService::dormant(
            Resources {
                home: self.home.service_reference(),
                storage: self.storage.clone(),
                runtime,
            },
            limits,
        );
        service.shared.state.lock().unwrap().live = true;
        service
    }

    pub fn activate(&self) -> CasTurnSource {
        let source = support::exact_cas::establish_turn(
            &self.home,
            self.storage.clone(),
            id(30),
            self.turn,
            timestamp(101),
        );
        self.publish(&source, SourceEventPayload::TurnActivated, 102);
        source
    }

    pub fn publish(&self, source: &CasTurnSource, payload: SourceEventPayload, at: u64) {
        let state = self
            .storage
            .turn_state(&self.home, self.turn, point_limit())
            .unwrap()
            .unwrap();
        let gate = self
            .storage
            .input_gate(&self.home, id(30), point_limit())
            .unwrap()
            .unwrap();
        let event = LiveSourceEvent::new(
            id(30),
            self.turn,
            state.revision(),
            gate.revision(),
            SourceEventSequence::new(state.source_event_count() + 1).unwrap(),
            Some(source.clone()),
            payload,
            timestamp(at),
        )
        .unwrap();
        self.interest
            .as_ref()
            .unwrap()
            .with_activity_for_test(
                ActivityQuerySource::new(id(30), self.turn),
                |qualification| {
                    let mut command = HomeCommand::new(self.home.home_revision().unwrap());
                    command
                        .add(self.storage.admit_live_source_event(
                            self.storage.revision(&self.home).unwrap(),
                            event,
                            qualification,
                        ))
                        .unwrap();
                    assert!(matches!(
                        self.home.execute(command),
                        CommandOutcome::Committed {
                            later_failure: None,
                            ..
                        }
                    ));
                },
            )
            .unwrap();
    }

    pub fn add_command(&self, source: &CasTurnSource, index: u8) {
        let at = 110 + u64::from(index);
        let item = SyndicItemId::from_bytes([index; 16]);
        let frame = ProviderItemFrameV1::new(
            ProviderFrameOrdinalV1::FIRST,
            CasItemId::new(format!("activity-command-{index}")).unwrap(),
            ProviderItemObservationV1::Started {
                observed_at: ProviderLifecycleTimestampMsV1::new(at),
                item: ProviderItemV1::CommandExecution(ProviderCommandExecutionV1 {
                    command: ProviderTextV1::inline(format!("command-{index}")),
                    cwd: ProviderTextV1::inline("C:/workspace"),
                    process_id: None,
                    source: ProviderCommandSourceV1::Agent,
                    status: ProviderCommandStatusV1::InProgress,
                    command_actions: Vec::new(),
                    aggregated_output: None,
                    exit_code: None,
                    duration_ms: None,
                }),
            },
        );
        let sealed = support::exact_cas::stage_item_frame(
            &self.home,
            self.storage.clone(),
            self.turn,
            item,
            source,
            frame,
        );
        self.publish(
            source,
            SourceEventPayload::ItemFrame {
                item_id: item,
                frame: Box::new(sealed),
            },
            at,
        );
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.interest.take();
        assert!(self.owner.shutdown());
    }
}
