//! Shared pure-data types used across the Beryl workspace.
//!
//! ```rust
//! use beryl_model::conversation::{
//!     ConversationThreadId, ConversationThreadTokenUsageSnapshot,
//!     ConversationTokenUsageBreakdown, ConversationTurnId, RegisteredConversationThread,
//!     WorkspaceConversationState,
//! };
//! use beryl_model::provenance::{MutationProvenance, MutationSource};
//! use beryl_model::semantic_graph::{
//!     SemanticGraph, SemanticGraphPatch, SemanticGraphPatchOp, SemanticNodeDraft,
//!     SemanticNodeFacets, SemanticNodeId, ThreadRefDraft, ThreadRefId,
//! };
//! use beryl_model::workspace::{derive_workspace_slug, BerylWorkspaceManifest, WorkspaceId};
//!
//! let execution_target = WorkspaceId::host_windows(r"C:\work\beryl");
//! assert!(execution_target.display_label().contains("host-windows"));
//!
//! let semantic = BerylWorkspaceManifest::untitled(1, 1_700_000_000_000);
//! assert_eq!(semantic.title(), "Untitled 1");
//! assert_eq!(derive_workspace_slug("My Project").unwrap().as_str(), "my-project");
//!
//! let mut conversation = WorkspaceConversationState::default();
//! let thread = RegisteredConversationThread::new(
//!     ConversationThreadId::new("thread_1"),
//!     execution_target.clone(),
//!     "Explain the renderer",
//!     Some("Renderer".to_string()),
//!     1,
//!     2,
//! );
//! conversation
//!     .select_runtime(execution_target.runtime_mode().clone())
//!     .unwrap();
//! conversation
//!     .designate_primary_execution_target(&execution_target)
//!     .unwrap();
//! conversation.remember_thread(thread);
//! conversation
//!     .record_thread_token_usage_snapshot(
//!         &ConversationThreadId::new("thread_1"),
//!         ConversationThreadTokenUsageSnapshot::new(
//!             ConversationTurnId::new("turn_1"),
//!             ConversationTokenUsageBreakdown::new(0, 1200, 300, 100, 1600),
//!             ConversationTokenUsageBreakdown::new(0, 2400, 600, 200, 3200),
//!             Some(200_000),
//!             1_700_000_000_000,
//!         ),
//!     )
//!     .unwrap();
//!
//! let provenance = MutationProvenance::new(
//!     "operator",
//!     1_700_000_000_000,
//!     MutationSource::conversation_turn(
//!         ConversationThreadId::new("thread_1"),
//!         ConversationTurnId::new("turn_1"),
//!     ),
//!     Some(100),
//! )
//! .unwrap();
//! let node_id = SemanticNodeId::new("renderer").unwrap();
//! let mut graph = SemanticGraph::default();
//! graph
//!     .apply_patch(&SemanticGraphPatch::new(vec![
//!         SemanticGraphPatchOp::UpsertNode {
//!             node: SemanticNodeDraft::new(
//!                 node_id.clone(),
//!                 "Renderer",
//!                 "Capture renderer work.",
//!                 SemanticNodeFacets::topic(),
//!                 None,
//!             ),
//!             provenance: provenance.clone(),
//!         },
//!         SemanticGraphPatchOp::SetHardParent {
//!             child_id: node_id.clone(),
//!             parent_id: None,
//!             index: None,
//!             provenance: provenance.clone(),
//!         },
//!         SemanticGraphPatchOp::UpsertThreadRef {
//!             thread_ref: ThreadRefDraft::new(
//!                 ThreadRefId::new("renderer_thread").unwrap(),
//!                 node_id.clone(),
//!                 ConversationThreadId::new("thread_1"),
//!                 WorkspaceId::host_windows(r"C:\work\beryl"),
//!                 "Renderer thread",
//!             ),
//!             provenance,
//!         },
//!     ]))
//!     .unwrap();
//! assert_eq!(graph.node(&node_id).unwrap().title(), "Renderer");
//! assert_eq!(graph.root_node_ids(), &[node_id]);
//! ```

pub mod conversation;
pub mod provenance;
pub mod semantic_graph;
pub mod workspace;
