use super::*;
use beryl_model::BindingRevision;

pub struct ContextObservationTestHarness {
    connection: ConnectionGeneration,
    runtime: RuntimeId,
    process: CasProcessGeneration,
    registrations: usize,
}

pub struct ContextProjectionTestHandle {
    key: LoadedThreadKey,
    owner: SyndicThreadId,
    generation: CasLoadedSessionGeneration,
    token: LeaseToken,
    revision: BindingRevision,
}

impl ContextObservationTestHarness {
    pub fn new(runtime: RuntimeId, process: CasProcessGeneration) -> Self {
        Self {
            connection: allocate_connection_generation().unwrap(),
            runtime,
            process,
            registrations: 0,
        }
    }

    pub fn register(
        &mut self,
        owner: SyndicThreadId,
        thread: CasThreadId,
        model: Option<&str>,
        revision: BindingRevision,
    ) -> ContextProjectionTestHandle {
        assert!(self.registrations < 8, "test harness projection bound");
        let key = LoadedThreadKey {
            runtime_id: self.runtime,
            process_generation: self.process,
            cas_thread_id: thread,
        };
        let (generation, token) = register_new(
            key.clone(),
            self.connection,
            owner,
            beryl_backend::ThreadSessionMetadata {
                model: model.map(str::to_owned),
                ..Default::default()
            },
        )
        .unwrap();
        bind_context(
            &key,
            self.connection,
            owner,
            generation,
            token,
            None,
            revision,
            false,
        )
        .unwrap();
        self.registrations += 1;
        ContextProjectionTestHandle {
            key,
            owner,
            generation,
            token,
            revision,
        }
    }

    pub fn unregister(&mut self, handle: ContextProjectionTestHandle) {
        assert_eq!(
            release_exact(
                &handle.key,
                self.connection,
                handle.owner,
                handle.generation,
                handle.token
            )
            .unwrap(),
            ReleaseDisposition::Last
        );
        self.registrations -= 1;
    }

    pub fn advance(
        &self,
        handle: &mut ContextProjectionTestHandle,
        revision: BindingRevision,
        proven: bool,
    ) {
        bind_context(
            &handle.key,
            self.connection,
            handle.owner,
            handle.generation,
            handle.token,
            Some(handle.revision),
            revision,
            proven,
        )
        .unwrap();
        handle.revision = revision;
    }

    pub fn observe(&self, handle: &ContextProjectionTestHandle, wire: &str) {
        struct Sink<'a> {
            harness: &'a ContextObservationTestHarness,
            handle: &'a ContextProjectionTestHandle,
        }
        impl beryl_backend::OrderedTurnStreamSink for Sink<'_> {
            fn submit(
                &mut self,
                operation: beryl_backend::OrderedTurnStreamOperation,
            ) -> Result<
                beryl_backend::OrderedTurnStreamCompletion,
                beryl_backend::OrderedTurnStreamSubmitError,
            > {
                match operation {
                    beryl_backend::OrderedTurnStreamOperation::ThreadContextObservation(
                        observation,
                    ) => {
                        observe_context(
                            &self.handle.key,
                            self.harness.connection.get(),
                            self.handle.owner,
                            self.handle.generation,
                            observation,
                        )
                        .unwrap();
                    }
                    beryl_backend::OrderedTurnStreamOperation::AccountQuotaObservation(quota) => {
                        observe_quota(self.harness.connection.get(), quota).unwrap()
                    }
                    _ => panic!("unexpected context test operation"),
                }
                Ok(beryl_backend::OrderedTurnStreamCompletion::Applied)
            }
        }
        beryl_backend::lifecycle_test_support::decode_provider_json_for_test(
            wire.as_bytes(),
            1,
            &mut Sink {
                harness: self,
                handle,
            },
        )
        .unwrap();
    }

    pub fn read(
        &self,
        handle: &ContextProjectionTestHandle,
    ) -> Option<beryl_backend::ThreadContextObservation> {
        read_context(
            self.connection,
            handle.owner,
            &handle.key.cas_thread_id,
            handle.revision,
        )
        .unwrap()
        .map(|snapshot| snapshot.observation)
    }

    pub fn hold(&self, handle: &ContextProjectionTestHandle) -> impl Fn() -> bool + use<> {
        let snapshot = read_context(
            self.connection,
            handle.owner,
            &handle.key.cas_thread_id,
            handle.revision,
        )
        .unwrap()
        .unwrap();
        move || context_is_current(&snapshot.stamp)
    }

    pub fn interest(&self) -> (Option<u64>, Option<String>, Option<u64>) {
        lock()
            .unwrap()
            .connection_authority_counts
            .get(&self.connection)
            .map(|authority| authority.context.diagnostics())
            .unwrap_or((None, None, None))
    }

    pub fn retire(&self) {
        invalidate_connection(self.connection).unwrap();
    }
}

impl Drop for ContextObservationTestHarness {
    fn drop(&mut self) {
        self.retire();
    }
}
