//! Syndic-owned durable thread, draft, history, and projection storage.
//!
//! The package owns bounded Syndic lifecycle, ordering, immutable parent and
//! context, transcript-position, and CAS-lineage proof values. Its durable
//! domain uses the one physical [`beryl_home_store`] database and never exposes
//! Fjall, encoded records, or a second store to callers. Cross-package stable
//! identities and revisions remain owned by [`beryl_model`].
//! [`idle_submission_max_footprint`] and [`accepted_input_promotion_max_footprint`] expose the
//! checked maximum V5 record/key/value shapes for the two durable new-turn starts. Callers pass
//! those opaque typed participants to `beryl-home-store`; they never supply a byte estimate.
//!
//! ```
//! let footprint = syndic_storage::idle_submission_max_footprint()?;
//! let _ = footprint;
//! # Ok::<(), beryl_home_store::DurableStartFootprintError>(())
//! ```
//!
//! # Registration and validation
//!
//! [`SyndicStorage::register`], [`SyndicStorage::reacquire`], and
//! [`SyndicStorage::reacquire_candidate`] are routine declaration-, family-,
//! exact-type-, and generation-bound operations. They never scan Syndic
//! application records. Use
//! [`SyndicStorage::register_with_schema_validation`] only for an explicit
//! schema-validation boundary. Whole-home scrub and corruption-evidence paths
//! remain store-owned exhaustive operations; ordinary typed reads still reject
//! malformed records when they encounter them.
//!
//! # Provider-item frames
//!
//! [`ProviderItemFrameV1`] is the closed typed boundary for published provider history. Use
//! [`encode_provider_item_frame_v1`] for bounded chunk emission and
//! [`validate_streaming_provider_item_frame_v1`] for constant-resident validation. The compiled
//! `provider_item_frame` example demonstrates the convenience encode/decode path. Incoming
//! provider observations first use the separate unpublished staging boundary below.
//!
//! # Unpublished provider observations
//!
//! [`ProviderObservationStager`] durably validates the selected provider schema before a sealed
//! observation can be bound to its admitted route. A [`ProviderObservationStageCallback`] returns
//! [`beryl_home_store::CommandOutcome`] for each offered exact batch; the stager then returns its
//! typed [`ProviderObservationStageOutcome`] behind its semantic `Result` boundary. Consuming seal
//! instead returns [`ProviderObservationSealOutcome`], whose indeterminate variant retains the
//! inert consumed stager inside a move-only [`ProviderObservationSealCustodyGuard`] until its sole
//! terminal installation. In particular, a committed batch may retain a later failure, while an
//! indeterminate batch retains its opaque reconciliation custody. [`inspect_provider_observation`]
//! consumes that route-bound
//! authority and extracts fixed-resident identity and lifecycle facts. It can then be consumed into
//! a private-constructible [`ProviderObservationIssue`] candidate; live-source publication still
//! proves the supplied closed conflict reason against the exact durable source frontier.
//!
//! ```no_run
//! use beryl_home_store::{CommandOutcome, HomeOpenOptions, HomeSchemaVersion, HomeStore};
//! use beryl_model::{CasThreadId, CasTurnId, ProviderObservationId};
//! use syndic_storage::{
//!     ProviderField, ProviderObservationBegin, ProviderObservationControl,
//!     ProviderObservationIssueReason, ProviderObservationItemKind,
//!     ProviderObservationItemLifecycle, ProviderObservationRoute,
//!     ProviderObservationSealOutcome, ProviderObservationStageBatch, ProviderObservationStager,
//!     ProviderObservationStageOutcome,
//!     ProviderObservationStagingBytes, ProviderScalar, ProviderValueContext,
//!     SyndicPointReadLimit, SyndicStorage, inspect_provider_observation,
//! };
//!
//! # fn example(path: &std::path::Path) -> Result<(), Box<dyn std::error::Error>> {
//! let mut home = HomeStore::open(HomeOpenOptions::new(path, HomeSchemaVersion::CURRENT))?;
//! let syndic = SyndicStorage::register(&mut home)?;
//! let mut commit = |batch: &ProviderObservationStageBatch| -> CommandOutcome {
//!     home.execute_current(syndic.current_stage_provider_observation_batch(batch.clone()))
//! };
//! let mut staging = match ProviderObservationStager::begin(
//!     ProviderObservationId::from_bytes([7; 16]),
//!     ProviderObservationBegin::Item {
//!         lifecycle: ProviderObservationItemLifecycle::Completed,
//!         kind: ProviderObservationItemKind::ContextCompaction,
//!     },
//!     &mut commit,
//! )? {
//!     ProviderObservationStageOutcome::NotCommitted { evidence } => return Err(evidence.into()),
//!     ProviderObservationStageOutcome::Committed { value, receipt, later_failure } => {
//!         let _exact_receipt = receipt;
//!         if let Some(failure) = later_failure {
//!             return Err(failure.into());
//!         }
//!         value
//!     }
//!     ProviderObservationStageOutcome::Indeterminate { failure, reconciliation } => {
//!         reconciliation.install();
//!         return Err(failure.into());
//!     }
//! };
//! match staging.control(
//!     ProviderObservationControl::Scalar {
//!         context: ProviderValueContext::Field(ProviderField::LifecycleObservedAt),
//!         value: ProviderScalar::Unsigned(42),
//!     },
//!     &mut commit,
//! )? {
//!     ProviderObservationStageOutcome::NotCommitted { evidence } => return Err(evidence.into()),
//!     ProviderObservationStageOutcome::Committed { receipt, later_failure, .. } => {
//!         let _exact_receipt = receipt;
//!         if let Some(failure) = later_failure {
//!             return Err(failure.into());
//!         }
//!     }
//!     ProviderObservationStageOutcome::Indeterminate { failure, reconciliation } => {
//!         reconciliation.install();
//!         return Err(failure.into());
//!     }
//! }
//! let item = ProviderValueContext::Field(ProviderField::ItemId);
//! match staging.control(ProviderObservationControl::BeginField(item), &mut commit)? {
//!     ProviderObservationStageOutcome::NotCommitted { evidence } => return Err(evidence.into()),
//!     ProviderObservationStageOutcome::Committed { receipt, later_failure, .. } => {
//!         let _exact_receipt = receipt;
//!         if let Some(failure) = later_failure { return Err(failure.into()); }
//!     }
//!     ProviderObservationStageOutcome::Indeterminate { failure, reconciliation } => {
//!         reconciliation.install();
//!         return Err(failure.into());
//!     }
//! }
//! match staging.fragment(
//!     ProviderObservationStagingBytes::new(item, b"provider-item")?,
//!     &mut commit,
//! )? {
//!     ProviderObservationStageOutcome::NotCommitted { evidence } => return Err(evidence.into()),
//!     ProviderObservationStageOutcome::Committed { receipt, later_failure, .. } => {
//!         let _exact_receipt = receipt;
//!         if let Some(failure) = later_failure { return Err(failure.into()); }
//!     }
//!     ProviderObservationStageOutcome::Indeterminate { failure, reconciliation } => {
//!         reconciliation.install();
//!         return Err(failure.into());
//!     }
//! }
//! match staging.control(ProviderObservationControl::EndField(item), &mut commit)? {
//!     ProviderObservationStageOutcome::NotCommitted { evidence } => return Err(evidence.into()),
//!     ProviderObservationStageOutcome::Committed { receipt, later_failure, .. } => {
//!         let _exact_receipt = receipt;
//!         if let Some(failure) = later_failure { return Err(failure.into()); }
//!     }
//!     ProviderObservationStageOutcome::Indeterminate { failure, reconciliation } => {
//!         reconciliation.install();
//!         return Err(failure.into());
//!     }
//! }
//! let sealed = match staging.seal(&mut commit)? {
//!     ProviderObservationSealOutcome::NotCommitted { evidence } => return Err(evidence.into()),
//!     ProviderObservationSealOutcome::Committed { value, receipt, later_failure } => {
//!         let _exact_receipt = receipt;
//!         if let Some(failure) = later_failure { return Err(failure.into()); }
//!         value
//!     }
//!     ProviderObservationSealOutcome::Indeterminate { failure, custody } => {
//!         custody.install();
//!         return Err(failure.into());
//!     }
//! };
//! assert_eq!(sealed.identity(), ProviderObservationId::from_bytes([7; 16]));
//! let route = ProviderObservationRoute::new(
//!     CasThreadId::new("provider-thread")?,
//!     CasTurnId::new("provider-turn")?,
//! );
//! let bound = sealed.bind(route.clone(), route)?;
//! let inspected = inspect_provider_observation(
//!     &syndic,
//!     &home,
//!     bound,
//!     SyndicPointReadLimit::new(4_096)?,
//! )?;
//! let issue = inspected.into_issue(ProviderObservationIssueReason::MissingItemStart);
//! assert_eq!(issue.reason(), ProviderObservationIssueReason::MissingItemStart);
//! # home.close()?;
//! # Ok(())
//! # }
//! ```
//!
//! # Domain registration and reacquisition
//!
//! ```no_run
//! use beryl_home_store::{HomeOpenOptions, HomeSchemaVersion, HomeStore};
//! use syndic_storage::SyndicStorage;
//!
//! # fn example(path: &std::path::Path) -> Result<(), Box<dyn std::error::Error>> {
//! let mut home = HomeStore::open(HomeOpenOptions::new(path, HomeSchemaVersion::CURRENT))?;
//! let syndic = SyndicStorage::register(&mut home)?;
//! assert_eq!(syndic.revision(&home)?.get(), 1);
//! home.close()?;
//! # Ok(())
//! # }
//! ```
//!
//! A recovered generation is reacquired with [`SyndicStorage::reacquire`]; old
//! handles cannot authorize reads or command receipts in the replacement generation.
//!
//! # Ordinary threads and durable drafts
//!
//! Thread creation contributes one mutually consistent thread/draft aggregate plus its canonical
//! empty content to one revision-checked [`beryl_home_store::HomeCommand`]. The caller owns natural
//! identities and can reconcile an ambiguous admitted result with
//! [`SyndicStorage::thread_creation_status`]. [`SyndicStorage::current_draft`]
//! returns an index-stabilized thread/draft pair; [`DraftPayloadUpdate::prepare`]
//! returns an explicit no-change result instead of scheduling an unchanged write.
//!
//! ```no_run
//! use beryl_home_store::{CommandOutcome, HomeCommand, HomeOpenOptions, HomeSchemaVersion, HomeStore};
//! use beryl_model::{
//!     ExecutionBinding, PathFlavor, RootId, RuntimeId, RuntimeMode, RuntimeNativePath,
//!     SyndicDraftId, SyndicThreadId,
//! };
//! use syndic_storage::{CreateThread, SyndicStorage, SyndicTimestamp};
//!
//! # fn example(path: &std::path::Path) -> Result<(), Box<dyn std::error::Error>> {
//! let mut home = HomeStore::open(HomeOpenOptions::new(path, HomeSchemaVersion::CURRENT))?;
//! let syndic = SyndicStorage::register(&mut home)?;
//! let creation = CreateThread::ordinary(
//!     SyndicThreadId::from_bytes([1; 16]),
//!     SyndicDraftId::from_bytes([2; 16]),
//!     ExecutionBinding::new(
//!         RuntimeId::from_bytes([3; 16]),
//!         RootId::from_bytes([4; 16]),
//!         RuntimeNativePath::from_admitted(
//!             RuntimeMode::host(),
//!             PathFlavor::Windows,
//!             "C:\\beryl-syndic-example",
//!         )?,
//!     ),
//!     SyndicTimestamp::from_unix_millis(1),
//! );
//! let mut command = HomeCommand::new(home.home_revision()?);
//! command.add(syndic.create_thread(syndic.revision(&home)?, creation))?;
//! match home.execute(command) {
//!     CommandOutcome::NotCommitted { evidence } => return Err(evidence.into()),
//!     CommandOutcome::Committed { receipt, later_failure } => {
//!         let _exact_receipt = receipt;
//!         if let Some(failure) = later_failure { return Err(failure.into()); }
//!     }
//!     CommandOutcome::Indeterminate { failure, reconciliation } => {
//!         reconciliation.install();
//!         return Err(failure.into());
//!     }
//! }
//! # Ok(())
//! # }
//! ```
//!
//! # Compact catalog summaries
//!
//! [`SyndicStorage::prepare_thread_catalog_summary`] stabilizes canonical title, attributes,
//! execution, history-summary, and thread sources. An exact-current result can contribute a
//! validation-only guard to a heterogeneous home command. A prepared replacement exposes the
//! exact summary that the same command will publish, while retaining opaque mutation authority.
//!
//! ```no_run
//! use beryl_home_store::{CommandOutcome, HomeCommand, HomeStore};
//! use beryl_model::SyndicThreadId;
//! use syndic_storage::{SyndicStorage, ThreadCatalogSummaryPreparation};
//!
//! # fn rebuild(
//! #     home: &HomeStore,
//! #     syndic: SyndicStorage,
//! #     thread: SyndicThreadId,
//! # ) -> Result<(), Box<dyn std::error::Error>> {
//! match syndic.prepare_thread_catalog_summary(home, thread)? {
//!     Some(ThreadCatalogSummaryPreparation::ExactCurrent(exact)) => {
//!         let _summary = exact.summary();
//!     }
//!     Some(ThreadCatalogSummaryPreparation::PreparedReplacement(prepared)) => {
//!         let _post_commit_summary = prepared.replacement().clone();
//!         let mut command = HomeCommand::new(home.home_revision()?);
//!         command.add(syndic.rebuild_thread_catalog_summary(prepared))?;
//!         match home.execute(command) {
//!             CommandOutcome::NotCommitted { evidence } => return Err(evidence.into()),
//!             CommandOutcome::Committed { receipt, later_failure } => {
//!                 let _exact_receipt = receipt;
//!                 if let Some(failure) = later_failure { return Err(failure.into()); }
//!             }
//!             CommandOutcome::Indeterminate { failure, reconciliation } => {
//!                 reconciliation.install();
//!                 return Err(failure.into());
//!             }
//!         }
//!     }
//!     None => {}
//! }
//! # Ok(())
//! # }
//! ```
//!
//! # Input admission and exact reconciliation
//!
//! Admission requests name the exact current content and all caller-owned result
//! identities. After an ambiguous admitted outcome, callers verify the same home
//! and use [`SyndicStorage::idle_submission_status`] or
//! [`SyndicStorage::accepted_input_status`] before publishing success or retrying.
//! Non-idle commits persist a complete [`AcceptedInputAdmissionProof`]; its checked
//! construction keeps source and replacement drafts distinct, while
//! [`AcceptedInputRecord`] binds the accepted identity to the source draft. Exact
//! accepted reconciliation remains valid across later mutable route, gate, lifecycle,
//! and replacement-draft descendants.
//!
//! ```no_run
//! use beryl_home_store::{CommandOutcome, HomeCommand, HomeStore};
//! use beryl_model::{InputGateRevision, SyndicDraftId, SyndicItemId};
//! use syndic_storage::{
//!     IdleSubmission, SyndicCurrentDraft, SyndicStorage, SyndicTimestamp,
//! };
//!
//! # fn admit(
//! #     home: &HomeStore,
//! #     syndic: SyndicStorage,
//! #     current: &SyndicCurrentDraft,
//! # ) -> Result<(), Box<dyn std::error::Error>> {
//! let submission = IdleSubmission::new(
//!     current.thread().id(),
//!     current.thread().revision(),
//!     current.draft().id(),
//!     current.draft().revision(),
//!     current.draft().content(),
//!     InputGateRevision::new(1)?,
//!     SyndicDraftId::from_bytes([3; 16]),
//!     SyndicItemId::from_bytes([4; 16]),
//!     None,
//!     SyndicTimestamp::from_unix_millis(2),
//! );
//! let mut command = HomeCommand::new(home.home_revision()?);
//! command.add(syndic.submit_idle_draft(syndic.revision(home)?, submission))?;
//! match home.execute(command) {
//!     CommandOutcome::NotCommitted { evidence } => return Err(evidence.into()),
//!     CommandOutcome::Committed { receipt, later_failure } => {
//!         let _exact_receipt = receipt;
//!         if let Some(failure) = later_failure { return Err(failure.into()); }
//!     }
//!     CommandOutcome::Indeterminate { failure, reconciliation } => {
//!         reconciliation.install();
//!         return Err(failure.into());
//!     }
//! }
//! # Ok(())
//! # }
//! ```
//!
//! # Exact accepted-input delivery
//!
//! An `UnknownTerminal` source outcome atomically changes the selected live steering generation
//! into durable queue-only [`NextTurnReason::UnknownTerminal`] authority and moves the input gate
//! to `AwaitingTerminal`. Inputs admitted during that interval are also next-turn work. Late item
//! evidence cannot restore steering authority. An exact later activation for the same CAS turn
//! opens a fresh empty steering generation while retaining all interval work as next-turn work; an
//! exact proven-terminal event instead advances the gate to terminal-history finalization without
//! retargeting that work. Startup classifies an awaiting-terminal gate as possible-dispatch active
//! authority, so recovery must retire the active binding before the queued work can progress.
//!
//! Accepted steering delivery names stable source intent: the thread, accepted input, exact source
//! leaf revision, and exact steering target. [`BeginAcceptedInputDelivery`] claims only `Admitted`
//! or proven-retryable live steering. A proven pre-dispatch failure uses
//! [`RetryAcceptedInputDelivery`], exact provider success uses
//! [`CompleteAcceptedInputDelivery`], and possible dispatch without an authoritative response can
//! be resolved only by [`AbandonActiveBinding`]. That atomic transition retires the active CAS
//! projection while retaining terminal `DeliveryUnknown` accepted-input history without replay
//! authority. Structured non-steerable rejection remains the separate [`SteeringRejection`]
//! transition. Local worker or connection capacity pressure does not mutate durable route state.
//!
//! The serialized mutation resolves the current compatible same-generation gate, route head, and
//! generation atomically. A successful transition persists their actual pre-mutation facts in an
//! [`AcceptedRouteLeafTransitionProof`], allowing the corresponding
//! [`AcceptedInputDeliveryTransitionStatus`] read to classify an ambiguous result as `Prior`,
//! `Exact`, or `Collision`. Exact reconciliation accepts monotonic compatible descendants and does
//! not require a quiet shared gate or route revision.
//!
//! Active abandonment separately persists an [`AcceptedRouteAbandonmentProof`] in the
//! projection-loss generation. Its stable intent names the source binding, route generation, loss
//! target, and generic or exact-rejected-input disposition. The witness records the gate and route
//! actually consumed by the atomic mutation, so those two outcomes cannot authenticate each other.
//!
//! ```
//! use beryl_model::{AcceptedInputRevision, SyndicAcceptedInputId, SyndicThreadId};
//! use syndic_storage::{BeginAcceptedInputDelivery, SteeringTargetProof};
//!
//! # fn build_claim(
//! #     target: SteeringTargetProof,
//! # ) -> Result<(), beryl_model::RevisionError> {
//! let claim = BeginAcceptedInputDelivery::new(
//!     SyndicThreadId::from_bytes([1; 16]),
//!     SyndicAcceptedInputId::from_bytes([3; 16]),
//!     AcceptedInputRevision::new(4)?,
//!     target,
//! );
//! assert_eq!(claim.expected_input_revision().get(), 4);
//! # Ok(())
//! # }
//! ```
//!
//! # Durable exact stop authority
//!
//! [`AdmitStopOperation`] atomically binds one caller nonce to the exact active Syndic turn,
//! binding, execution snapshot, loaded process generation, and CAS turn. The selected steering
//! generation becomes compact [`NextTurnReason::Stop`] authority without rewriting its leaves,
//! while input admitted during the stop remains separate ordered next-turn work.
//! [`JoinStopCause`] monotonically joins another owner, and [`ClaimStopDispatch`] records the sole
//! caller-generated attempt before any backend request byte may be issued.
//! [`StopOperationRecord::cause_first_revisions`] exposes the four fixed persisted
//! first-publication slots: every admission cause names [`StopOperationRevision::FIRST`], while a
//! later join names its immediate successor revision. [`StopOperationRecord::dispatch_claim`]
//! returns the retained [`StopDispatchClaimWitness`] with both the exact live source revision and
//! attempt identity. [`StopOperationRecord::causes`] and [`StopOperationRecord::attempt`] are
//! derived conveniences, not persistence or reconciliation authority.
//!
//! [`SyndicStorage::stop_admission_read`] is the only public discovery boundary for stop
//! authority. Two coherent bounded passes authenticate the same complete target, reverse-index,
//! route-source, and live-stop facts used by reconciliation, then return either an exact
//! [`StopAdmissionCandidate`], the matching [`SyndicLiveStopOperation`], or a typed
//! [`StopAdmissionIneligibility`]. The candidate builds [`AdmitStopOperation`] from a caller nonce
//! and cause set without allowing a mismatched thread identity.
//!
//! A matching terminal event, [`SafelyReopenStopOperation`] after locally proven pre-byte failure,
//! or [`AbandonStopOperation`] after classified authority loss consumes live stop authority into a
//! permanent receipt. Startup returns [`DeliveryRecoveryCase::Stopping`] for either admitted or
//! dispatch-claimed work; it never creates retry authority. Each explicit storage transition has a
//! fixed-work `Prior`/`Exact`/`Collision` reconciliation read.
//!
//! ```no_run
//! use beryl_home_store::{CommandOutcome, HomeStore};
//! use beryl_model::SyndicThreadId;
//! use syndic_storage::{
//!     StopAdmissionRead, StopCause, StopCauseSet, StopOperationNonce, SyndicPointReadLimit,
//!     SyndicStorage,
//! };
//!
//! # fn discover(
//! #     home: &HomeStore,
//! #     syndic: SyndicStorage,
//! #     thread: SyndicThreadId,
//! # ) -> Result<(), Box<dyn std::error::Error>> {
//! if let StopAdmissionRead::Admissible(candidate) = syndic.stop_admission_read(
//!     home,
//!     thread,
//!     SyndicPointReadLimit::new(65_536)?,
//! )? {
//!     let request = candidate.admission(
//!         StopOperationNonce::from_bytes([7; 16]),
//!         StopCauseSet::from(StopCause::SelectedOperationControl),
//!     );
//!     match home.execute_current(syndic.current_admit_stop_operation(request)) {
//!         CommandOutcome::NotCommitted { evidence } => return Err(evidence.into()),
//!         CommandOutcome::Committed { receipt, later_failure } => {
//!             let _exact_receipt = receipt;
//!             if let Some(failure) = later_failure { return Err(failure.into()); }
//!         }
//!         CommandOutcome::Indeterminate { failure, reconciliation } => {
//!             reconciliation.install();
//!             return Err(failure.into());
//!         }
//!     }
//! }
//! # Ok(())
//! # }
//! ```
//!
//! ```no_run
//! use beryl_home_store::{CommandOutcome, HomeStore};
//! use syndic_storage::{
//!     AdmitStopOperation, StopOperationTransitionStatus, SyndicPointReadLimit, SyndicStorage,
//! };
//!
//! # fn reconcile(
//! #     home: &HomeStore,
//! #     syndic: SyndicStorage,
//! #     request: &AdmitStopOperation,
//! # ) -> Result<(), Box<dyn std::error::Error>> {
//! match syndic.stop_admission_status(
//!     home,
//!     request,
//!     SyndicPointReadLimit::new(65_536)?,
//! )? {
//!     StopOperationTransitionStatus::Prior => {
//!         match home.execute_current(syndic.current_admit_stop_operation(request.clone())) {
//!             CommandOutcome::NotCommitted { evidence } => return Err(evidence.into()),
//!             CommandOutcome::Committed { receipt, later_failure } => {
//!                 let _exact_receipt = receipt;
//!                 if let Some(failure) = later_failure { return Err(failure.into()); }
//!             }
//!             CommandOutcome::Indeterminate { failure, reconciliation } => {
//!                 reconciliation.install();
//!                 return Err(failure.into());
//!             }
//!         }
//!     }
//!     StopOperationTransitionStatus::Exact => {}
//!     StopOperationTransitionStatus::Collision => {
//!         return Err("stop admission authority changed".into());
//!     }
//! }
//! # Ok(())
//! # }
//! ```
//!
//! # Bounded active-steering discovery
//!
//! [`SyndicStorage::accepted_ready_source_page`] scans compact source authority globally in
//! `(thread, route generation)` order under one exact domain revision.
//! [`SyndicStorage::accepted_ready_candidate_page`] then scans one selected source in permanent
//! accepted order and returns only `Admitted` or `Retryable` leaf facts. Its cursor advances across
//! delivering, next-turn, and terminal rows, so an empty candidate page can still make bounded
//! progress. After reserving fixed worker capacity, a caller reopens one candidate through
//! [`SyndicStorage::ready_steering_input`] before claiming delivery.
//!
//! ```no_run
//! use beryl_home_store::{CursorReadLimits, HomeStore};
//! use syndic_storage::{
//!     ACCEPTED_READY_PAGE_MAX_BYTES, ACCEPTED_READY_PAGE_MAX_RECORDS, SyndicPointReadLimit,
//!     SyndicReadError, SyndicStorage,
//! };
//!
//! # fn discover(
//! #     home: &HomeStore,
//! #     syndic: SyndicStorage,
//! # ) -> Result<(), Box<dyn std::error::Error>> {
//! let revision = syndic.revision(home)?;
//! let sources = syndic.accepted_ready_source_page(
//!     home,
//!     revision,
//!     None,
//!     CursorReadLimits::new(
//!         ACCEPTED_READY_PAGE_MAX_RECORDS,
//!         ACCEPTED_READY_PAGE_MAX_BYTES,
//!     )?,
//! )?;
//! for source in sources.records() {
//!     let candidates = syndic.accepted_ready_candidate_page(
//!         home,
//!         *source,
//!         None,
//!         CursorReadLimits::new(
//!             ACCEPTED_READY_PAGE_MAX_RECORDS,
//!             ACCEPTED_READY_PAGE_MAX_BYTES,
//!         )?,
//!     )?;
//!     if let Some(candidate) = candidates.records().first() {
//!         let ready = syndic.ready_steering_input(
//!             home,
//!             candidate.input_id(),
//!             SyndicPointReadLimit::new(65_536)?,
//!         )?;
//!         let _ = ready;
//!     }
//! }
//! # Ok(())
//! # }
//! ```
//!
//! A later commit returns [`SyndicReadError::StaleAcceptedReadySourceScan`] for an older global
//! scan. Gate, route-head, source, or generation drift returns
//! [`SyndicReadError::StaleAcceptedReadyCandidateSource`]; a cursor from another exact source
//! revision returns [`SyndicReadError::InvalidAcceptedReadyCandidateCursor`].
//!
//! # Delivery restart discovery
//!
//! [`SyndicStorage::delivery_recovery_startup_page`] advances over physical input-gate keys without
//! a domain-revision fence, allowing an exclusive startup owner to mutate already visited threads.
//! Each compact [`DeliveryRecoverySource`] is reopened through
//! [`SyndicStorage::classify_delivery_recovery`] into one fixed-work [`DeliveryRecoveryCase`].
//! `FinalizingHistory` cases remain discoverable until bounded canonical-item and selected-
//! transcript convergence allows [`CompleteTerminalHistory`] to consume the observed gate or a
//! compatible descendant containing later queued admissions.
//! Independently, [`SyndicStorage::recovered_pending_page`] returns only proven undispatched
//! pending turns under one exact domain revision.
//!
//! ```no_run
//! use beryl_home_store::{CursorReadLimits, HomeStore};
//! use syndic_storage::{
//!     DELIVERY_RECOVERY_GATE_PAGE_MAX_BYTES, DELIVERY_RECOVERY_GATE_PAGE_MAX_RECORDS,
//!     DeliveryRecoveryCase, SyndicPointReadLimit, SyndicStorage,
//! };
//!
//! # fn discover(
//! #     home: &HomeStore,
//! #     syndic: SyndicStorage,
//! # ) -> Result<(), Box<dyn std::error::Error>> {
//! let page = syndic.delivery_recovery_startup_page(
//!     home,
//!     None,
//!     CursorReadLimits::new(
//!         DELIVERY_RECOVERY_GATE_PAGE_MAX_RECORDS,
//!         DELIVERY_RECOVERY_GATE_PAGE_MAX_BYTES,
//!     )?,
//! )?;
//! for source in page.records() {
//!     if let DeliveryRecoveryCase::Active(active) = syndic.classify_delivery_recovery(
//!         home,
//!         source,
//!         SyndicPointReadLimit::new(65_536)?,
//!     )? {
//!         let abandonment = active.generic_abandonment(
//!             "startup recovered possible dispatch",
//!             active.minimum_timestamp(),
//!         )?;
//!         let _ = abandonment;
//!     }
//! }
//!
//! let revision = syndic.revision(home)?;
//! let pending = syndic.recovered_pending_page(
//!     home,
//!     revision,
//!     None,
//!     CursorReadLimits::new(
//!         DELIVERY_RECOVERY_GATE_PAGE_MAX_RECORDS,
//!         DELIVERY_RECOVERY_GATE_PAGE_MAX_BYTES,
//!     )?,
//!     SyndicPointReadLimit::new(65_536)?,
//! )?;
//! for recovered in pending.records() {
//!     let _ = (recovered.thread_id(), recovered.turn_id());
//! }
//! # Ok(())
//! # }
//! ```
//!
//! # Bounded accepted next-turn promotion
//!
//! [`SyndicStorage::accepted_next_source_page`] scans compact next-turn source authority in
//! `(thread, route generation)` order under one domain revision. The source-local
//! [`SyndicStorage::accepted_next_candidate_page`] advances across terminal history and returns at
//! most the earliest effective candidate. Both pages clamp caller limits to
//! [`ACCEPTED_NEXT_PAGE_MAX_RECORDS`] and [`ACCEPTED_NEXT_PAGE_MAX_BYTES`] without retaining the
//! queue. The caller reserves bounded worker capacity before discovery and skips later sources for
//! a thread whose exact gate is no longer idle. A complete idle scan whose source still claims
//! next-turn work is invariant corruption, not an ordinary empty result.
//!
//! [`PromoteAcceptedInput`] combines that opaque candidate with caller-owned fresh turn and item
//! identities. [`SyndicStorage::promote_accepted_input`] contributes the atomic Syndic mutation;
//! the Beryl-home caller combines it with the accepted-input-to-submitted-item asset-owner transfer
//! in the same `SyncAll` home command when the sealed content references assets. After an ambiguous
//! commit outcome, [`SyndicStorage::accepted_input_promotion_status`] authenticates the immutable
//! promotion witness and successor identities across compatible monotonic descendants, then
//! classifies the result as [`AcceptedInputPromotionStatus::Prior`],
//! [`AcceptedInputPromotionStatus::Exact`], or [`AcceptedInputPromotionStatus::Collision`] before
//! any retry. A later valid accepted admission against the promoted pending gate does not erase an
//! exact promotion result.
//!
//! Old source revisions and mismatched continuations return the corresponding
//! [`SyndicReadError::StaleAcceptedNextSourceScan`],
//! [`SyndicReadError::StaleAcceptedNextCandidateSource`],
//! [`SyndicReadError::InvalidAcceptedNextSourceCursor`],
//! [`SyndicReadError::InvalidAcceptedNextCandidateSource`], or
//! [`SyndicReadError::InvalidAcceptedNextCandidateCursor`]. The compiled
//! `accepted_next_promotion` example demonstrates bounded discovery and request construction while
//! leaving cross-domain command composition to the Beryl-home owner.
//!
//! A delivery worker first uses [`SyndicStorage::ready_steering_input`] to resolve one `Admitted`
//! or `Retryable` input and its exact live CAS authority with twelve stabilized point reads.
//! Delayed steering lifecycle correlation uses
//! [`SyndicStorage::delivering_steering_input`] to resolve one immutable input and the exact
//! currently steerable CAS thread/turn target without scanning accepted-route pages.
//!
//! ```no_run
//! use beryl_home_store::HomeStore;
//! use beryl_model::SyndicAcceptedInputId;
//! use syndic_storage::{SyndicPointReadLimit, SyndicReadError, SyndicStorage};
//!
//! # fn resolve(
//! #     home: &HomeStore,
//! #     syndic: SyndicStorage,
//! #     input_id: SyndicAcceptedInputId,
//! # ) -> Result<(), SyndicReadError> {
//! if let Some(input) = syndic.delivering_steering_input(
//!     home,
//!     input_id,
//!     SyndicPointReadLimit::new(65_536).expect("nonzero point-read limit"),
//! )? {
//!     let cas_thread = input.target().pending().cas_thread_id();
//!     let cas_turn = input.target().cas_turn_id();
//!     let loaded_generation = input.loaded_generation();
//!     let runtime = input.execution().runtime_id();
//!     let _ = (cas_thread, cas_turn, loaded_generation, runtime);
//! }
//! # Ok(())
//! # }
//! ```
//!
//! # Bounded submitted text
//!
//! [`SyndicStorage::sealed_content_text_range`] reads an exact marker-free sealed
//! [`ContentReference`] through logical text spans rather than assembling its encoded composer.
//! The caller can append each UTF-8-safe page directly into the one backend-owned string.
//! [`SyndicStorage::prove_sealed_content_text_segment`] authenticates and scans one complete
//! marker-bounded ordered-piece interval before returning an opaque segment. Bounded calls to
//! [`SyndicStorage::sealed_content_text_segment_range`] then require that proof and return absolute
//! UTF-8 continuations without rescanning or crossing a marker.
//!
//! ```no_run
//! use beryl_home_store::HomeStore;
//! use syndic_storage::{ContentReference, SyndicReadError, SyndicStorage};
//!
//! # fn submitted_text(
//! #     home: &HomeStore,
//! #     syndic: SyndicStorage,
//! #     content: ContentReference,
//! # ) -> Result<Option<String>, SyndicReadError> {
//! let mut text = String::new();
//! let mut offset = 0;
//! loop {
//!     let Some(page) = syndic.sealed_content_text_range(home, content, offset, 16_384)? else {
//!         return Ok(None);
//!     };
//!     text.push_str(page.text());
//!     match page.next_offset() {
//!         Some(next) => offset = next,
//!         None => return Ok(Some(text)),
//!     }
//! }
//! # }
//! ```
//!
//! ```no_run
//! use beryl_home_store::HomeStore;
//! use syndic_storage::{
//!     ContentReference, SyndicContentTextSegmentBoundary, SyndicReadError, SyndicStorage,
//! };
//!
//! # fn submitted_segment(
//! #     home: &HomeStore,
//! #     syndic: SyndicStorage,
//! #     content: ContentReference,
//! #     after_marker: Option<SyndicContentTextSegmentBoundary>,
//! # ) -> Result<Option<String>, SyndicReadError> {
//! let Some(segment) = syndic.prove_sealed_content_text_segment(home, content, after_marker)? else {
//!     return Ok(None);
//! };
//! let mut text = String::new();
//! let mut offset = segment.start();
//! loop {
//!     let Some(page) = syndic.sealed_content_text_segment_range(
//!         home,
//!         &segment,
//!         offset,
//!         16_384,
//!     )? else {
//!         return Ok(None);
//!     };
//!     text.push_str(page.text());
//!     match page.next_offset() {
//!         Some(next) => offset = next,
//!         None => return Ok(Some(text)),
//!     }
//! }
//! # }
//! ```
//!
//! # Canonical live history
//!
//! [`LiveSourceEvent`] admits one exact normalized event together with its
//! canonical content, turn-state frontier, and transcript invalidation. Exact
//! replay is reported as [`SyndicMutationError::SourceEventAlreadyAdmitted`];
//! different data at the same sequence and a future sequence remain distinct
//! failures. A proven-terminal event closes source admission and moves the input gate to
//! [`InputGateState::FinalizingHistory`]. [`FinalizeNextTurnItem`] can then advance only the
//! contiguous frontier derived from events already stored. [`CompleteTerminalHistory`] changes
//! only the current compatible descendant of its observed gate to idle after proving the same
//! terminal committed tail, a semantically current selected-transcript build, and the durable
//! item-convergence fixed point. Path-neutral queued admissions preserve their route accounting
//! and do not supersede an active or completed transcript generation.
//! [`HistorySummaryRecord`] carries its own monotonic projection revision, independent of the
//! broader thread revision, so compact catalog source fences observe draft-only activity changes.
//!
//! Provider-created item events carry [`SourceEventPayload::ItemFrame`] only after the frame has
//! crossed bounded ProviderItemV1 staging and structural validation. The event therefore names one
//! exact sealed frame reference rather than duplicating text or retaining a generic payload.
//!
//! # Recovery projection preflight and restart
//!
//! Recovery assembly is read-only and has two explicit scopes. Before admission, callers use the
//! complete current selected path so an unavailable history or model budget leaves the current
//! draft untouched. After restart with an already admitted pending selected turn, callers use its
//! parent scope so the pending input is not injected as prior history. A ready assembly contains
//! only compact totals, digest, path, and revision proof; text is replayed later through one opaque
//! sequential cursor. Empty prefixes return [`RecoveryAssembly::NativeEmptyPrefix`] without model
//! metadata.
//!
//! ```no_run
//! use std::num::NonZeroUsize;
//! use beryl_home_store::HomeStore;
//! use beryl_model::SyndicThreadId;
//! use beryl_stream::PagePool;
//! use syndic_storage::{
//!     RecoveryAssembly, RecoveryProjectionRequest, RecoveryProjectionScope, SelectedPathProof,
//!     SyndicStorage,
//! };
//!
//! # fn prepare(
//! #     home: &HomeStore,
//! #     syndic: SyndicStorage,
//! #     thread: SyndicThreadId,
//! #     current_selected_path: SelectedPathProof,
//! #     pending_selected_path: SelectedPathProof,
//! # ) -> Result<(), Box<dyn std::error::Error>> {
//! let page_pool = PagePool::new(
//!     NonZeroUsize::new(65_536).unwrap(),
//!     NonZeroUsize::new(1).unwrap(),
//! )?;
//!
//! let pre_admission = RecoveryProjectionRequest::for_current_selected_path(
//!     thread,
//!     current_selected_path,
//!     Some(200_000),
//! );
//! assert_eq!(
//!     pre_admission.scope(),
//!     RecoveryProjectionScope::CurrentSelectedPath
//! );
//! let assembled = syndic.prepare_recovery_projection(home, pre_admission)?;
//! if let RecoveryAssembly::Ready(proof) = &assembled {
//!     let mut cursor = syndic.open_recovery_cursor(home, *proof)?;
//!     let mut page_lease = page_pool.try_lease()?;
//!     loop {
//!         let Some(page) = syndic.read_recovery_cursor_page(
//!             home,
//!             &mut cursor,
//!             page_lease,
//!             65_536,
//!         )? else {
//!             break;
//!         };
//!         let ordinal = page.sequence_ordinal();
//!         let role = page.role();
//!         let declared_bytes = page.declared_item_utf8_bytes();
//!         let offset = page.item_offset();
//!         let item_terminal = page.item_terminal();
//!         let sequence_terminal = page.sequence_terminal();
//!         let text_bytes = page.text().len();
//!         page_lease = page.into_page_lease();
//!         // Transfer this exact lease and closed-role metadata into the backend source.
//!         let _ = (
//!             ordinal,
//!             role,
//!             declared_bytes,
//!             offset,
//!             text_bytes,
//!             item_terminal,
//!             sequence_terminal,
//!         );
//!     }
//! }
//!
//! // If admission is later found durable after restart, read its new selected-path proof and use:
//! let pending_parent = RecoveryProjectionRequest::for_pending_selected_turn_parent(
//!     thread,
//!     pending_selected_path,
//!     Some(200_000),
//! );
//! assert_eq!(
//!     pending_parent.scope(),
//!     RecoveryProjectionScope::PendingSelectedTurnParent
//! );
//! let _ = (assembled.source_revision(), pending_parent);
//! # Ok(())
//! # }
//! ```
//!
//! An assembly's source revision records read provenance only. A later proof publication uses the
//! then-current domain revision and exact current thread, selected-path, and binding expected
//! revisions rather than treating that older global revision as mutation authority.
//!
//! # Pure lineage values
//!
//! ```
//! use beryl_model::{SyndicPathDigest, SyndicThreadId, SyndicTurnId, ThreadRevision};
//! use syndic_storage::{
//!     CasLineageProof, CasRepresentedPrefixProof, ConversationParent, NativeCasLineage,
//!     ThreadLineageDepth, ThreadLineageProof, root_thread_lineage_digest,
//! };
//!
//! let thread = SyndicThreadId::from_bytes([1; 16]);
//! let thread_lineage = ThreadLineageProof::new(
//!     None,
//!     None,
//!     ThreadLineageDepth::FIRST,
//!     root_thread_lineage_digest(thread),
//! );
//! let tail = SyndicTurnId::from_bytes([7; 16]);
//! let represented_prefix = CasRepresentedPrefixProof::new(
//!     Some(tail),
//!     ThreadRevision::new(3)?,
//!     SyndicPathDigest::from_bytes([9; 32]),
//! );
//! let lineage = CasLineageProof::native(NativeCasLineage::Continuation, represented_prefix)?;
//!
//! assert_eq!(ConversationParent::Turn(tail).turn(), Some(tail));
//! assert_eq!(thread_lineage.depth(), ThreadLineageDepth::FIRST);
//! assert_eq!(lineage.established_prefix(), represented_prefix);
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! The storage flow is also kept as the compiling Cargo example `domain`.
#![forbid(unsafe_code)]

mod catalog_title;
mod codec;
mod compaction;
mod content;
mod domain;
mod error;
mod footprint;
mod membership;
mod mutation;
mod native_projection;
mod projection;
mod provider_item;
mod provider_observation;
mod read;
mod record;
mod recovery;
mod selected_path;
mod terminal_history;
#[cfg(feature = "test-faults")]
pub mod test_faults;
mod thread_lineage;
mod validation;
mod value;

pub use compaction::{
    LIFECYCLE_CONTINUATION_TEXT, derive_compaction_snapshot_id,
    derive_lifecycle_continuation_item_id, derive_lifecycle_continuation_turn_id,
    prepare_lifecycle_continuation_content,
};
pub use content::{ComposerContentAssembler, PreparedContent};
pub use domain::SyndicStorage;
pub use error::{RecoveryBudgetKind, RecoveryProjectionError, SyndicReadError, SyndicRecordError};
pub use footprint::{accepted_input_promotion_max_footprint, idle_submission_max_footprint};
pub use mutation::{
    AbandonActiveBinding, AbandonCompactionOperation, AbandonStopOperation,
    AcceptGeneratedThreadTitle, AcceptedInputAdmission, AcceptedInputPromotionStatus,
    ActivateBinding, ActiveCasTurnPublicationStatus, AdmitCompactionOperation, AdmitStopOperation,
    AdvanceItemProjectionBuild, AdvanceTranscriptBuild, ArchiveBranchDiscussionThread,
    BeginAcceptedInputDelivery, BindingPublicationStatus, CONTENT_APPEND_MAX_CHUNKS,
    CancelBindingActivation, CancelReplacementEdit, ClaimCompactionDispatch, ClaimStopDispatch,
    CompactionProviderEvent, CompleteAcceptedInputDelivery, CompleteTerminalHistory, ContentAppend,
    ContentBuild, CreateThread, CreateThreadError, DraftPayloadUpdate, DraftPayloadUpdateDecision,
    ExactRejectedInputDelivery, FinalizeNextTurnItem, FreezeNextTurnItem, IdleSubmission,
    InputAdmissionStatus, JoinStopCause, LiveSourceEvent, LiveSourceEventStatus,
    PROVIDER_FRAME_STAGE_MAX_NARRATIVE_SPANS, PreparedProviderFrame, PromoteAcceptedInput,
    ProviderCompletionComparisonMutationError, ProviderFrameMutationError,
    ProviderFramePreparationError, ProviderFramePreparationPlan, ProviderFrameStageBatch,
    ProviderFrameStageBatchError, ProviderFrameStageBatchState, ProviderFrameStageCallback,
    ProviderFrameStageError, ProviderFrameStageOutcome, ProviderObservationMutationError,
    PublishActiveCasTurn, PublishActivityChildHandoff, PublishCompactionProviderEvent,
    PublishCompactionRequestDisposition, PublishStaleBinding, PublishThreadUsage,
    PublishUnboundBinding, PublishValidBinding, RetryAcceptedInputDelivery,
    SafelyReopenStopOperation, SealLifecycleContinuationContent, SettleCompactionOperation,
    SettleLifecycleCompaction, StartItemProjectionBuild, StartReplacementEdit,
    StartTranscriptBuild, SteeringRejection, SyndicMutationError, ThreadCreationStatus,
    prepare_provider_frame, stage_provider_frame,
};
pub use native_projection::{
    NativeProjectionBasis, NativeProjectionError, NativeProjectionPlan, NativeProjectionRequest,
    NativeProjectionSource, NativeProjectionUnavailable,
};
pub use provider_item::*;
pub use provider_observation::*;
pub(crate) use read::AcceptedNextCandidateBasis;
pub use read::{
    ACCEPTED_NEXT_PAGE_MAX_BYTES, ACCEPTED_NEXT_PAGE_MAX_RECORDS, ACCEPTED_READY_PAGE_MAX_BYTES,
    ACCEPTED_READY_PAGE_MAX_RECORDS, ACCEPTED_ROUTE_PAGE_MAX_RECORDS,
    ACCEPTED_ROUTE_PAGE_MAX_STORED_BYTES, AcceptedInputDeliveryTransitionStatus,
    AcceptedNextCandidate, AcceptedNextCandidateCursor, AcceptedNextCandidatePage,
    AcceptedNextSource, AcceptedNextSourceCursor, AcceptedNextSourcePage, AcceptedReadyCandidate,
    AcceptedReadyCandidateCursor, AcceptedReadyCandidatePage, AcceptedReadySourceCursor,
    AcceptedReadySourcePage, AcceptedRouteCursor, AcceptedRouteEffectiveState, AcceptedRouteEntry,
    AcceptedRoutePage, ActiveDeliveryRecovery, ActivityQueryCursor, ActivityQueryPage,
    ActivityQuerySourceCursor, ActivityQuerySourcePage, CompactionAdmissionCandidate,
    CompactionAdmissionIneligibility, CompactionAdmissionRead, CompactionRecoveryCase,
    CompactionRequestTransitionStatus, DELIVERY_RECOVERY_GATE_PAGE_MAX_BYTES,
    DELIVERY_RECOVERY_GATE_PAGE_MAX_RECORDS, DeliveryRecoveryCase,
    DeliveryRecoveryClassificationError, DeliveryRecoverySource, DeliveryRecoveryStartupCursor,
    DeliveryRecoveryStartupPage, ExactThreadCatalogSummary,
    PreparedThreadCatalogSummaryReplacement, QUERY_PAGE_MAX_RECORDS, QUERY_PAGE_MAX_STORED_BYTES,
    RecoveredPendingCursor, RecoveredPendingPage, RecoveredPendingSource, StopAdmissionCandidate,
    StopAdmissionIneligibility, StopAdmissionRead, StopOperationTransitionStatus,
    SyndicCaptureItem, SyndicCaptureTextRangeRead, SyndicContentTextRangeRead,
    SyndicContentTextSegment, SyndicContentTextSegmentBoundary, SyndicContentTextSegmentRangeRead,
    SyndicCurrentBinding, SyndicDeliveringSteeringInput, SyndicLiveStopOperation, SyndicPage,
    SyndicPointReadLimit, SyndicReadySteeringInput, SyndicResolvedImageLabelOriginSpan,
    SyndicResourceRangeRead, ThreadCatalogSummaryPreparation, ThreadLineageCursor,
    ThreadLineageEntry, ThreadLineageHead, ThreadLineagePage,
};
pub use read::{SyndicCurrentDraft, SyndicThreadTail};
pub use record::*;
pub use recovery::{
    RECOVERY_CURSOR_PAGE_MAX_UTF8_BYTES, RecoveryAssembly, RecoveryCursor, RecoveryCursorPage,
    RecoveryItemSequenceRole, RecoveryProjection, RecoveryProjectionRequest,
    RecoveryProjectionScope,
};

pub use value::{
    AcceptedInputLifecycle, AcceptedInputOrdinal, AcceptedRouteGeneration, AcceptedRouteRevision,
    ActivityQueryRevision, ActivityWorkPeriod, AssistantMessagePhase, BindingLifecycle,
    CasLineageMode, CasLineageProof, CasRepresentedPrefixProof, CompactionAbandonmentReason,
    CompactionAttemptNonce, CompactionMarkerLifecycle, CompactionMarkerObservation,
    CompactionOperationId, CompactionOperationNonce, CompactionOperationRevision,
    CompactionProviderSequence, CompactionRequestDisposition, CompactionSettlement,
    CompactionSettlementReceiptCommitment, CompactionThreadStatus, ComposerAtomOrdinal,
    ContentChunkOrdinal, ContentEncoding, ContentLifecycle, ContentPieceOrdinal,
    ContextEnvelopeRevision, ConversationParent, CurrentTranscriptEntryProof,
    DISCUSSION_CONTEXT_MAX_BYTES, DiscussionContextDescriptor, DiscussionContextEnvelope,
    DiscussionContextRange, DiscussionContextSource, DiscussionContextText,
    DiscussionContextVersion, ImageLabelFrontier, ImageLabelOrdinal, InputGateState,
    InputMarkerOrdinal, ItemProjectionGeneration, ItemSourceEventOrdinal, NativeCasLineage,
    NextTurnReason, PendingSteeringTargetProof, ProjectionLifecycle, ProjectionOrdinal,
    ProviderControlOrdinal, ProviderItemBuildRevision, ProviderItemKind, ProviderItemLifecycle,
    ProviderNarrativeGeneration, ProviderObservationIssueReason, ProviderOperationKind,
    RecoveredInjectionProof, RecoveryItemCount, RecoveryProjectionVersion, RecoveryUtf8ByteCount,
    ResourceOrdinal, SelectedPathProof, SourceEventSequence, SteeringTargetProof,
    StopAbandonmentReason, StopAttemptNonce, StopCause, StopCauseFirstRevisions,
    StopCauseFirstRevisionsError, StopCauseSet, StopCauseSetError, StopDispatchClaimWitness,
    StopOperationId, StopOperationNonce, StopOperationRevision, SyndicConnectionGeneration,
    SyndicTimestamp, SyndicValueError, ThreadAttributesRevision, ThreadLineageDepth,
    ThreadUsageRevision, TranscriptGeneration, TranscriptPosition, TurnDepth, TurnEndStatus,
    TurnIncompleteReason, TurnItemOrdinal, TurnKind, TurnLifecycle, TurnStateRevision,
    TurnTerminalOutcome, UnsupportedHistoryReason,
};
