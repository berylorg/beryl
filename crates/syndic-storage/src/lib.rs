//! Syndic-owned durable thread, draft, history, and projection storage.
//!
//! The package owns bounded Syndic lifecycle, ordering, immutable parent and
//! context, transcript-position, and CAS-lineage proof values. Its durable
//! domain uses the one physical [`beryl_home_store`] database and never exposes
//! Fjall, encoded records, or a second store to callers. Cross-package stable
//! identities and revisions remain owned by [`beryl_model`].
//!
//! # Provider-item frames
//!
//! [`ProviderItemFrameV1`] is the closed typed provider observation boundary. Use
//! [`encode_provider_item_frame_v1`] for bounded chunk emission and
//! [`validate_streaming_provider_item_frame_v1`] for constant-resident validation. The compiled
//! `provider_item_frame` example demonstrates the convenience encode/decode path.
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
//! use beryl_home_store::{HomeCommand, HomeOpenOptions, HomeSchemaVersion, HomeStore};
//! use beryl_model::{SyndicDraftId, SyndicThreadId};
//! use syndic_storage::{CreateThread, SyndicStorage, SyndicTimestamp};
//!
//! # fn example(path: &std::path::Path) -> Result<(), Box<dyn std::error::Error>> {
//! let mut home = HomeStore::open(HomeOpenOptions::new(path, HomeSchemaVersion::CURRENT))?;
//! let syndic = SyndicStorage::register(&mut home)?;
//! let creation = CreateThread::ordinary(
//!     SyndicThreadId::from_bytes([1; 16]),
//!     SyndicDraftId::from_bytes([2; 16]),
//!     SyndicTimestamp::from_unix_millis(1),
//! );
//! let mut command = HomeCommand::new(home.home_revision()?);
//! command.add(syndic.create_thread(syndic.revision(&home)?, creation))?;
//! home.execute(command)?;
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
//!
//! ```no_run
//! use beryl_home_store::{HomeCommand, HomeStore};
//! use beryl_model::{InputGateRevision, SyndicDraftId, SyndicItemId};
//! use syndic_storage::{
//!     AdmissionMarkers, IdleSubmission, SyndicCurrentDraft, SyndicStorage, SyndicTimestamp,
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
//!     AdmissionMarkers::default(),
//!     SyndicTimestamp::from_unix_millis(2),
//! );
//! let mut command = HomeCommand::new(home.home_revision()?);
//! command.add(syndic.submit_idle_draft(syndic.revision(home)?, submission))?;
//! home.execute(command)?;
//! # Ok(())
//! # }
//! ```
//!
//! # Exact accepted-input delivery
//!
//! Accepted steering delivery is revision-fenced at both the accepted input and its thread gate.
//! [`BeginAcceptedInputDelivery`] claims only `Admitted` or proven-retryable live steering. A
//! proven pre-dispatch failure uses [`RetryAcceptedInputDelivery`], exact provider success uses
//! [`CompleteAcceptedInputDelivery`], and possible dispatch without an authoritative response can
//! be resolved only by [`AbandonActiveBinding`]. That atomic transition retires the active CAS
//! projection while retaining terminal `DeliveryUnknown` accepted-input history without replay
//! authority. Structured non-steerable rejection remains the separate [`SteeringRejection`]
//! transition.
//!
//! ```
//! use beryl_model::{
//!     AcceptedInputRevision, InputGateRevision, SyndicAcceptedInputId, SyndicThreadId,
//! };
//! use syndic_storage::BeginAcceptedInputDelivery;
//!
//! let claim = BeginAcceptedInputDelivery::new(
//!     SyndicThreadId::from_bytes([1; 16]),
//!     InputGateRevision::new(2)?,
//!     SyndicAcceptedInputId::from_bytes([3; 16]),
//!     AcceptedInputRevision::new(4)?,
//! );
//! assert_eq!(claim.expected_input_revision().get(), 4);
//! # Ok::<(), beryl_model::RevisionError>(())
//! ```
//!
//! # Bounded submitted text
//!
//! [`SyndicStorage::sealed_content_text_range`] reads an exact marker-free sealed
//! [`ContentReference`] through logical text spans rather than assembling its encoded composer.
//! The caller can append each UTF-8-safe page directly into the one backend-owned string.
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
//! # Canonical live history
//!
//! [`LiveSourceEvent`] admits one exact normalized event together with its
//! canonical content, turn-state frontier, and transcript invalidation. Exact
//! replay is reported as [`SyndicMutationError::SourceEventAlreadyAdmitted`];
//! different data at the same sequence and a future sequence remain distinct
//! failures. A proven-terminal event closes source admission, after which
//! [`FinalizeNextTurnItem`] can advance only the contiguous frontier derived
//! from events already stored.
//!
//! ```
//! use beryl_model::{InputGateRevision, SyndicItemId, SyndicThreadId, SyndicTurnId};
//! use syndic_storage::{
//!     AssistantMessagePhase, LiveSourceEvent, SourceEventPayload, SourceEventSequence,
//!     SourceTextItemKind, SyndicTimestamp, TurnStateRevision,
//! };
//!
//! let item = SyndicItemId::from_bytes([3; 16]);
//! let event = LiveSourceEvent::new(
//!     SyndicThreadId::from_bytes([1; 16]),
//!     SyndicTurnId::from_bytes([2; 16]),
//!     TurnStateRevision::FIRST,
//!     InputGateRevision::new(2)?,
//!     SourceEventSequence::FIRST,
//!     None,
//!     SourceEventPayload::TextItemStarted {
//!         item_id: item,
//!         cas_item_id: None,
//!         kind: SourceTextItemKind::Assistant(AssistantMessagePhase::Unknown),
//!     },
//!     SyndicTimestamp::from_unix_millis(3),
//! )?;
//!
//! assert_eq!(event.sequence(), SourceEventSequence::FIRST);
//! assert_eq!(event.payload().item_id(), Some(item));
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! # Recovery projection preflight and restart
//!
//! Recovery assembly is read-only and has two explicit scopes. Before admission, callers use the
//! complete current selected path so an unavailable history or model budget leaves the current
//! draft untouched. After restart with an already admitted pending selected turn, callers use its
//! parent scope so the pending input is not injected as prior history. Empty prefixes return
//! [`RecoveryAssembly::NativeEmptyPrefix`] without requiring model metadata.
//!
//! ```no_run
//! use beryl_home_store::HomeStore;
//! use beryl_model::SyndicThreadId;
//! use syndic_storage::{
//!     RecoveryProjectionError, RecoveryProjectionRequest, RecoveryProjectionScope,
//!     SelectedPathProof, SyndicStorage,
//! };
//!
//! # fn prepare(
//! #     home: &HomeStore,
//! #     syndic: SyndicStorage,
//! #     thread: SyndicThreadId,
//! #     current_selected_path: SelectedPathProof,
//! #     pending_selected_path: SelectedPathProof,
//! # ) -> Result<(), RecoveryProjectionError> {
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
//! use beryl_model::{SyndicPathDigest, SyndicTurnId, ThreadRevision};
//! use syndic_storage::{
//!     CasLineageProof, CasRepresentedPrefixProof, ConversationParent, NativeCasLineage,
//! };
//!
//! let tail = SyndicTurnId::from_bytes([7; 16]);
//! let represented_prefix = CasRepresentedPrefixProof::new(
//!     Some(tail),
//!     ThreadRevision::new(3)?,
//!     SyndicPathDigest::from_bytes([9; 32]),
//! );
//! let lineage = CasLineageProof::native(NativeCasLineage::Continuation, represented_prefix)?;
//!
//! assert_eq!(ConversationParent::Turn(tail).turn(), Some(tail));
//! assert_eq!(lineage.established_prefix(), represented_prefix);
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! The storage flow is also kept as the compiling Cargo example `domain`.
#![forbid(unsafe_code)]

mod codec;
mod content;
mod domain;
mod error;
mod membership;
mod mutation;
mod native_projection;
mod projection;
mod provider_item;
mod read;
mod record;
mod recovery;
mod selected_path;
#[cfg(feature = "test-faults")]
pub mod test_faults;
mod validation;
mod value;

pub use content::{ComposerContentAssembler, PreparedContent};
pub use domain::SyndicStorage;
pub use error::{RecoveryBudgetKind, RecoveryProjectionError, SyndicReadError, SyndicRecordError};
pub use mutation::{
    AbandonActiveBinding, AcceptedInputAdmission, ActivateBinding, ActiveCasTurnPublicationStatus,
    AdmissionMarkers, AdvanceItemProjectionBuild, AdvanceTranscriptBuild,
    BeginAcceptedInputDelivery, BindingPublicationStatus, CONTENT_APPEND_MAX_CHUNKS,
    CancelBindingActivation, CancelReplacementEdit, CompleteAcceptedInputDelivery, ContentAppend,
    ContentBuild, CreateThread, CreateThreadError, DraftPayloadUpdate, DraftPayloadUpdateDecision,
    FinalizeNextTurnItem, FreezeNextTurnItem, IdleSubmission, InputAdmissionStatus,
    LiveSourceEvent, LiveSourceEventStatus, PublishActiveCasTurn, PublishStaleBinding,
    PublishUnboundBinding, PublishValidBinding, RetryAcceptedInputDelivery,
    StartItemProjectionBuild, StartReplacementEdit, StartTranscriptBuild, SteeringRejection,
    SyndicMutationError, ThreadCreationStatus,
};
pub use native_projection::{
    NativeProjectionBasis, NativeProjectionError, NativeProjectionPlan, NativeProjectionRequest,
    NativeProjectionSource, NativeProjectionUnavailable,
};
pub use provider_item::*;
pub use read::{
    SyndicCaptureItem, SyndicContentTextRangeRead, SyndicCurrentBinding, SyndicPage,
    SyndicPointReadLimit, SyndicResourceRangeRead, SyndicStoredRecord,
};
pub use read::{SyndicCurrentDraft, SyndicThreadTail};
pub use record::*;
pub use recovery::{
    RecoveryAssembly, RecoveryItem, RecoveryItemRole, RecoveryItemTextKind, RecoveryProjection,
    RecoveryProjectionRequest, RecoveryProjectionScope,
};

pub use value::{
    AcceptedInputDisposition, AcceptedInputLifecycle, AcceptedInputOrdinal, AssistantMessagePhase,
    BindingLifecycle, CasLineageMode, CasLineageProof, CasRepresentedPrefixProof,
    ComposerAtomOrdinal, ContentChunkOrdinal, ContentEncoding, ContentLifecycle,
    ContentPieceOrdinal, ContextEnvelopeRevision, ConversationParent, CurrentTranscriptEntryProof,
    DISCUSSION_CONTEXT_MAX_BYTES, DiscussionContextDescriptor, DiscussionContextEnvelope,
    DiscussionContextRange, DiscussionContextSource, DiscussionContextText,
    DiscussionContextVersion, ImageLabelOrdinal, InputGateState, InputMarkerOrdinal,
    ItemProjectionGeneration, ItemSourceEventOrdinal, NativeCasLineage, NextTurnReason,
    PendingSteeringTargetProof, ProjectionLifecycle, ProjectionOrdinal, ProviderItemKind,
    ProviderItemLifecycle, ProviderOperationKind, RecoveredInjectionProof, RecoveryItemCount,
    RecoveryProjectionVersion, RecoveryUtf8ByteCount, ResourceOrdinal, SelectedPathProof,
    SourceEventSequence, SteeringTargetProof, SyndicTimestamp, SyndicValueError,
    TranscriptGeneration, TranscriptPosition, TurnDepth, TurnEndStatus, TurnIncompleteReason,
    TurnItemOrdinal, TurnKind, TurnLifecycle, TurnStateRevision, TurnTerminalOutcome,
    UnsupportedHistoryReason,
};
