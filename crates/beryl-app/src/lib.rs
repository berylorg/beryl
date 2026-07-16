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
//! [`input_admission`] composes Syndic admission and every Beryl asset-reference
//! move into one durability-barrier command. The caller executes that command
//! before clearing or otherwise publishing acceptance of the editor projection.
//!
//! ```no_run
//! use beryl_app::input_admission::idle_submission_command;
//! use beryl_home_store::{CommitReceipt, HomeStore};
//! use beryl_model::{InputGateRevision, SyndicDraftId, SyndicItemId};
//! use beryl_state::AssetState;
//! use syndic_storage::{
//!     AdmissionMarkers, IdleSubmission, SyndicCurrentDraft, SyndicStorage, SyndicTimestamp,
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
//!     AdmissionMarkers::default(),
//!     SyndicTimestamp::from_unix_millis(2),
//! );
//! let command = idle_submission_command(home, syndic, assets, request)?;
//! Ok(home.execute(command)?)
//! # }
//! ```

mod branch_discussion_dynamic_tools;
pub mod cas_projection;
pub mod conversation_tools;
pub mod draft_persistence;
mod dynamic_tool_namespace;
pub mod input_admission;
mod lifecycle_dynamic_tools;
