//! Beryl's GPUI desktop-shell package boundary.
//!
//! Target modules are mounted here only when their owning rework checkpoint
//! supplies the complete target boundary.
//!
//! [`draft_persistence`] is the non-GPUI, single-current-draft coordinator. It
//! correlates exact binding, edit, timer, and request generations while leaving
//! durable record ownership to `syndic-storage`.
//!
//! ```
//! use beryl_app::draft_persistence::{
//!     DEFAULT_AUTOSAVE_SECONDS, DraftAutosavePublication,
//! };
//!
//! let initial = DraftAutosavePublication::absent_default();
//! assert_eq!(
//!     initial.interval().duration().as_secs(),
//!     DEFAULT_AUTOSAVE_SECONDS,
//! );
//! ```
//!
//! [`input_admission`] composes Syndic admission with the exact compact Beryl
//! asset-owner transfer, or validates both marker-free heads are absent, in one
//! durability-barrier command. The caller executes that command
//! before clearing or otherwise publishing acceptance of the editor projection.
//!
//! ```no_run
//! use beryl_app::input_admission::idle_submission_command;
//! use beryl_home_store::{CommitReceipt, HomeStore};
//! use beryl_model::{InputGateRevision, SyndicDraftId, SyndicItemId};
//! use beryl_state::AssetState;
//! use syndic_storage::{
//!     IdleSubmission, SyndicCurrentDraft, SyndicStorage, SyndicTimestamp,
//! };
//!
//! # fn admit(
//! #     home: &HomeStore,
//! #     syndic: SyndicStorage,
//! #     assets: AssetState,
//! #     current: &SyndicCurrentDraft,
//! # ) -> Result<CommitReceipt, Box<dyn std::error::Error>> {
//! let request = IdleSubmission::new(
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
//! let command = idle_submission_command(home, syndic, assets, request)?;
//! Ok(home.execute(command)?)
//! # }
//! ```
//!
//! [`catalog_projection::prepare_thread_catalog_projection`] performs one explicit non-GUI,
//! source-fenced join. A stale Syndic summary is rebuilt in the same home command as the Beryl
//! catalog row; exact agreement and a missing thread do not create a command.
//!
//! ```no_run
//! use beryl_app::catalog_projection::prepare_thread_catalog_projection;
//! use beryl_home_store::HomeStore;
//! use beryl_model::SyndicThreadId;
//! use beryl_state::BerylState;
//! use syndic_storage::SyndicStorage;
//!
//! # fn rebuild(
//! #     home: &HomeStore,
//! #     syndic: SyndicStorage,
//! #     state: BerylState,
//! #     thread: SyndicThreadId,
//! # ) -> Result<(), Box<dyn std::error::Error>> {
//! if let Some(command) =
//!     prepare_thread_catalog_projection(home, syndic, state, thread)?.into_command()
//! {
//!     home.execute(command)?;
//! }
//! # Ok(())
//! # }
//! ```
//!
//! [`cas_projection::CasProjectionCoordinator::execute_ordinary_turn`] consumes an exact loaded
//! projection only after the sealed submitted input has been reduced to one compact count/digest-
//! bound replay authority. Marker-bearing input merge-joins authenticated content markers with the
//! complete sealed asset-reference proof, verifies each first-occurrence sidecar, and projects its
//! exact Host or drive-backed WSL path one descriptor at a time. The caller supplies the current
//! [`beryl_state::AssetState`]; any preparation failure before activation returns the same loaded
//! projection for exact same-generation retry. Persistent home failure instead terminally disposes
//! the failed generation; it does not transfer projection authority into another service.
//!
//! [`cas_projection::ProjectionConnectionService::admit`] accepts a managed connector rather than
//! an externally initialized session. It fixes the home, generation, registered storage,
//! runtime/process identity, immutable foreground capacity, and the atomic pair of app worker
//! permits before the connector creates, initializes, and release-admits the candidate. Every failure
//! remains typed without exposing either the candidate or an admitted session.
//!
//! ```
//! use beryl_app::cas_projection::ProjectionServiceConfig;
//!
//! let config = ProjectionServiceConfig::try_new(128, 8).unwrap();
//! assert_eq!(
//!     config.foreground().pre_bind_control_capacity().get(),
//!     128,
//! );
//! assert_eq!(config.worker_capacity().get(), 8);
//! ```
//!
//! Construction also requires one process-shell
//! [`cas_projection::ScheduledOrdinaryExecutionProvider`]. Before a future next-turn promotion,
//! the service offers that provider an opaque admission token carrying the exact home, thread,
//! execution binding, protected worker permit, and same-thread flight. The provider must either
//! decline synchronously or complete the token with one admitted session checkout, an explicit
//! request policy, exact asset authority, and feature-owned dynamic-tool authority. Its
//! provisional lease is accepted only after the service revalidates the healthy home, live
//! service-owned session, runtime/process generation, and asset handle. Immediately before
//! promotion, the scheduler also acquires a one-shot reservation that linearizes with service
//! shutdown and exact connection retirement without holding either bounded gate across storage
//! work. Its `shutdown` implementation fences issuance and releases retained sessions before home
//! closure.
//!
//! ```
//! use std::time::Duration;
//!
//! use beryl_app::cas_projection::{
//!     OrdinaryTurnExecutionRequest, ScheduledOrdinaryRequestPolicy,
//! };
//! use beryl_backend::{ThreadStartOptions, TurnStartOptions};
//!
//! let policy = ScheduledOrdinaryRequestPolicy::new(
//!     ThreadStartOptions::persistent(),
//!     Some(2_000_000),
//!     Duration::from_secs(30),
//!     OrdinaryTurnExecutionRequest::new(
//!         TurnStartOptions::default(),
//!         Duration::from_secs(30),
//!     ),
//! );
//! assert!(!policy.thread_options().is_ephemeral());
//! assert_eq!(policy.model_context_window_tokens(), Some(2_000_000));
//! ```
//!
//! Exact live targets expose only feature-owned approvals and dynamic-tool operations. Target
//! polling is presentation-only: the sole connection driver owns each permission approval's exact
//! interruption obligation and drains it after backend denial, before exposing the enclosing
//! request result or advancing the next successful stream poll. Dropping the target cannot cancel
//! that obligation, while command and file-change denials require no second interruption.
//!
//! Exact stop admission is process-owned by the healthy-home
//! [`cas_projection::ProjectionConnectionService`]. Its selected-operation, diagnostic, and
//! window-close entry points perform storage and foreground-transport waits synchronously and
//! therefore run only on non-GPUI workers. Window close receives a non-cloneable
//! [`cas_projection::WindowCloseStopBarrier`] and may release its thread claim only after the
//! barrier proves terminal-history or authority-loss convergence. The same service owns lifecycle-
//! yield state so admitting an exact stop cancels only that turn's automatic phase continuation.
//! Exact soft interruption is the sole turn-stop path; it never becomes process control or coarse
//! cleanup, and no detached session or guessed process identity can exercise it.
//!
//! Persistent Beryl-home failure uses a separate process-local safety cut. Typed failed-health
//! observation and ordinary shutdown elect under the same master command gate; a failure that
//! wins invalidates every live command permit before the dedicated worker freezes exact router and
//! stop evidence. Stop admission, claim, and dispatch revalidate that generation inside the same
//! stop-state mutex used by freeze, so either already-admitted command work or the cut wins one
//! exact fence. A volatile request exists only when the original exact stop election transfers its
//! same command permit specifically to persistent failure immediately before any HomeStore writer
//! call, and remains single-use and bound to that exact target and sole driver. Broad persistent
//! failure, local absence or failure, and durable stop state cannot mint it. A writer-returned
//! `NotCommitted` outcome remains unavailable as app proof until Phase 100 preserves that typed
//! mutation result. Each pre-activation projection remains owned by its actual admitted worker,
//! while each router independently admits at most 64 targets. Router
//! mutations commit through their exact scoped gate permit. Consuming service close owns all
//! terminal I/O and joins: it settles retained queued work and local registry authority, shuts
//! down the old scheduler, context-compaction worker, connections, and execution provider, and
//! returns only bounded [`cas_projection::PersistentFailureTerminalEvidence`]. Possible-dispatch
//! outcomes remain explicitly unknown in that evidence. No service, connection, worker, home, or
//! publication authority crosses the failed-generation boundary, so the process supervisor
//! reports running-session recovery unavailable.
//!
//! Context compaction is likewise process-owned by
//! [`cas_projection::ProjectionConnectionService`]. Manual callers supply one exact thread and a
//! whole-second completion wait in the closed supported range. A timeout releases only that
//! caller; the shared coordinator continues to await exact provider terminal authority.
//!
//! ```
//! use std::time::Duration;
//! use beryl_app::cas_projection::ContextCompactionRequest;
//! use beryl_model::SyndicThreadId;
//!
//! let request = ContextCompactionRequest::new(
//!     SyndicThreadId::from_bytes([7; 16]),
//!     Duration::from_secs(30),
//! );
//! assert!(request.validate().is_ok());
//! ```
//!
//! Dynamic-tool envelopes use a separate ordered target operation. The connection broker selects
//! the canonical installed tool before arguments, streams one product into its feature-owned
//! builder, and dispatches the resulting non-cloneable request only through its narrowed
//! [`LifecycleYieldRequestHandler`] or [`BranchDiscussionResolutionRequestHandler`] boundary.
//! Unknown tools and product-schema or bounded-allocation failures retain no argument value and
//! receive their bounded response without entering either feature handler.
//!
//! ```
//! use beryl_app::cas_projection::LiveEventPoll;
//!
//! fn is_compact_approval(poll: &LiveEventPoll) -> bool {
//!     matches!(poll, LiveEventPoll::Approval(_))
//! }
//! ```

mod branch_discussion_dynamic_tools;
pub mod cas_projection;
pub mod catalog_projection;
pub mod conversation_tools;
pub mod draft_persistence;
mod dynamic_tool_namespace;
pub mod input_admission;
mod lifecycle_dynamic_tools;

pub use branch_discussion_dynamic_tools::{
    BranchDiscussionResolutionRequest, BranchDiscussionResolutionRequestHandler,
};
pub use lifecycle_dynamic_tools::{
    BerylLifecycleDynamicToolDispatch, LifecycleYieldOutcome, LifecycleYieldRequest,
    LifecycleYieldRequestHandler, dispatch_beryl_lifecycle_dynamic_tool_request,
};
