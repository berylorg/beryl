use super::support;
use beryl_home_store::{CommandOutcome, CursorReadLimits, HomeCommand, HomeStore};
use beryl_model::{CasItemId, SyndicItemId, SyndicThreadId, SyndicTurnId};
use support::{
    TestHome, draft_id,
    exact_cas::{
        admit_event, admit_item_frame, correlate_user_item, establish_turn, submit_current_draft,
    },
    id, timestamp,
};
use syndic_storage::*;

pub(super) struct Fixture {
    pub(super) home: HomeStore,
    pub(super) storage: SyndicStorage,
    _directory: TestHome,
    thread: SyndicThreadId,
    turn: SyndicTurnId,
    source: CasTurnSource,
    item: SyndicItemId,
}
pub(super) fn command_value(command: &str, status: ProviderCommandStatusV1) -> ProviderItemV1 {
    ProviderItemV1::CommandExecution(ProviderCommandExecutionV1 {
        command: ProviderTextV1::inline(command),
        cwd: ProviderTextV1::inline("/root"),
        process_id: None,
        source: ProviderCommandSourceV1::Agent,
        status,
        command_actions: vec![],
        aggregated_output: Some(ProviderTextV1::inline("private output")),
        exit_code: None,
        duration_ms: None,
    })
}
pub(super) fn change(path: &str, diff: &str) -> ProviderFileUpdateChangeV1 {
    ProviderFileUpdateChangeV1 {
        path: ProviderTextV1::inline(path),
        diff: ProviderTextV1::inline(diff),
        kind: ProviderPatchChangeKindV1::Update { move_path: None },
    }
}
impl Fixture {
    pub(super) fn new() -> Self {
        let directory = TestHome::new("activity-display");
        let mut candidate = support::open(directory.path());
        let storage = SyndicStorage::register(&mut candidate).unwrap();
        let home = candidate
            .prepare_publication(SyndicStorage::required_domains().unwrap())
            .unwrap()
            .publish()
            .unwrap();
        let thread = id(1);
        let mut command = HomeCommand::new(home.home_revision().unwrap());
        command
            .add(storage.create_thread(
                storage.revision(&home).unwrap(),
                CreateThread::ordinary(
                    thread,
                    draft_id(2),
                    support::exact_cas::execution_binding(),
                    timestamp(1),
                    DraftEditHistoryPolicyV1::new(65_536, 1).unwrap(),
                ),
            ))
            .unwrap();
        assert!(matches!(
            home.execute(command),
            CommandOutcome::Committed {
                later_failure: None,
                ..
            }
        ));
        let turn = submit_current_draft(
            &home,
            storage.clone(),
            thread,
            draft_id(3),
            SyndicItemId::from_bytes([4; 16]),
            "question",
            timestamp(3),
        );
        let source = establish_turn(&home, storage.clone(), thread, turn, timestamp(4));
        admit_event(
            &home,
            storage.clone(),
            thread,
            turn,
            &source,
            SourceEventPayload::TurnActivated,
            timestamp(4),
        );
        correlate_user_item(
            &home,
            storage.clone(),
            thread,
            turn,
            SyndicItemId::from_bytes([4; 16]),
            &source,
            timestamp(5),
        );
        Self {
            home,
            storage,
            _directory: directory,
            thread,
            turn,
            source,
            item: SyndicItemId::from_bytes([40; 16]),
        }
    }
    fn ordinal(&self) -> u64 {
        self.storage
            .canonical_item(
                &self.home,
                self.item,
                SyndicPointReadLimit::new(16_384).unwrap(),
            )
            .unwrap()
            .map_or(1, |item| {
                item.provider().unwrap().stream_state().next_ordinal().get()
            })
    }
    fn frame(&self, observation: ProviderItemObservationV1) -> ProviderItemFrameV1 {
        ProviderItemFrameV1::new(
            ProviderFrameOrdinalV1::new(self.ordinal()).unwrap(),
            CasItemId::new("display-item").unwrap(),
            observation,
        )
    }
    fn admit(&self, frame: ProviderItemFrameV1) {
        admit_item_frame(
            &self.home,
            self.storage.clone(),
            self.thread,
            self.turn,
            self.item,
            &self.source,
            frame,
            timestamp(10 + self.ordinal()),
        );
    }
    pub(super) fn start(&self, value: ProviderItemV1) {
        self.admit(self.frame(ProviderItemObservationV1::Started {
            observed_at: ProviderLifecycleTimestampMsV1::new(10),
            item: value,
        }));
    }
    pub(super) fn finish(&self, value: ProviderItemV1) {
        self.admit(self.frame(ProviderItemObservationV1::Completed {
            observed_at: ProviderLifecycleTimestampMsV1::new(20),
            item: value,
        }));
    }
    pub(super) fn delta(&self, delta: ProviderItemDeltaV1) {
        self.admit(self.frame(ProviderItemObservationV1::Delta(delta)));
    }
    pub(super) fn complete(&self, start: ProviderItemV1, finish: ProviderItemV1) {
        self.start(start);
        self.finish(finish);
    }
    pub(super) fn entry(&self) -> ActivityQueryEntryRecord {
        let head = self
            .storage
            .activity_query_head(
                &self.home,
                self.thread,
                SyndicPointReadLimit::new(16_384).unwrap(),
            )
            .unwrap()
            .unwrap();
        self.storage
            .activity_query_page(
                &self.home,
                &head,
                None,
                CursorReadLimits::new(64, 262_144).unwrap(),
            )
            .unwrap()
            .records()
            .iter()
            .find(|entry| entry.item_id() == self.item)
            .unwrap()
            .clone()
    }
    pub(super) fn extract(
        &self,
        entry: &ActivityQueryEntryRecord,
    ) -> (ActivityDisplaySource, usize) {
        let mut preparation = self
            .storage
            .prepare_activity_display(&self.home, entry)
            .unwrap();
        for steps in 1..100_000 {
            match preparation
                .step(&self.home, &self.storage, &|| false)
                .unwrap()
            {
                ActivityDisplayStep::Pending => {}
                ActivityDisplayStep::Complete(source) => return (source, steps),
            }
        }
        panic!("bounded source extraction did not finish")
    }
    pub(super) fn start_with_text_reference(
        &self,
        value: ProviderItemV1,
    ) -> ProviderTextReferenceV1 {
        let frame = self.frame(ProviderItemObservationV1::Started {
            observed_at: ProviderLifecycleTimestampMsV1::new(10),
            item: value,
        });
        self.admit_with_text_reference(frame)
    }
    pub(super) fn delta_with_text_reference(
        &self,
        delta: ProviderItemDeltaV1,
    ) -> ProviderTextReferenceV1 {
        self.admit_with_text_reference(self.frame(ProviderItemObservationV1::Delta(delta)))
    }
    fn admit_with_text_reference(&self, frame: ProviderItemFrameV1) -> ProviderTextReferenceV1 {
        struct Sink(Option<ProviderFrameTextSpanV1>);
        impl ProviderFrameSinkV1 for Sink {
            type Error = std::convert::Infallible;
            fn write_chunk(&mut self, _: &[u8]) -> Result<(), Self::Error> {
                Ok(())
            }
            fn write_text_span(
                &mut self,
                span: ProviderFrameTextSpanV1,
            ) -> Result<(), Self::Error> {
                if self.0.is_none() {
                    self.0 = Some(span);
                }
                Ok(())
            }
        }
        let prior = self
            .storage
            .canonical_item(
                &self.home,
                self.item,
                SyndicPointReadLimit::new(16_384).unwrap(),
            )
            .unwrap()
            .map_or(0, |item| {
                item.provider().unwrap().content().summary().encoded_bytes()
            });
        let mut sink = Sink(None);
        encode_provider_item_frame_v1(&frame, prior, &mut sink).unwrap();
        let span = sink.0.unwrap();
        self.admit(frame);
        ProviderTextReferenceV1::new(span.source_start(), span.source_end(), span.source_digest())
            .unwrap()
    }
}
