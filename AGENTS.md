Make sure you're following instructions in ~/.codex/AGENTS.md as well as workspace AGENTS.md.

---

# Vocabulary

- **Workspace project** - a Cargo package, a Gradle project, in other words an individually packagable, publishable and reusable artifact that is local to the workspace.
- **Subproject** - a non-root workspace project.
- **Aggregating directory** - a directory that is itself not a workspace project, but contains workspace projects.
- **Module** - a `mod` module in its own file in Rust, a source file in Java

# Documentation Contract

## Design

Every workspace project must include a design spec at `doc/design.md`.

Aggregating directories may omit `doc/`, but they may opt in with a shared design doc when decisions must be documented across multiple child workspace projects.

A workspace project's `doc/design.md` is owned by that workspace project.

- It may define decisions about the workspace project itself.
- It may define the workspace project's own public boundary contract: what it guarantees, what it requires, and what inputs/outputs are valid.
- It must not define internal policy for workspace projects it depends on.
- It must not define architecture or behavior policy for workspace projects that use it.
- It must not justify decisions based on unrelated systems outside this workspace project.
- It may mention workspace projects it depends on, but only to document assumptions or constraints about the APIs it consumes from them.
- The `## Non-goals` section is exempt from these scope limits.

Cross-workspace-project contract placement rule:
- Keep a contract in a workspace project's `doc/design.md` only when the contract is owned by that workspace project's boundary.
- If a contract sets shared rules between peer workspace projects, sibling workspace projects, parent-level orchestration, or anything not owned by one workspace project's boundary, put it in the nearest common parent `doc/design.md`.

Aggregating directory's `doc/design.md` files are only for those shared cross-workspace-project contracts. Do not include decisions that are internal to a single workspace project there.

Each subproject's `doc/design.md` must define a self-contained contract for that workspace project's boundary.

Do not include AGENTS/process-level guidance in workspace project design docs.

If a rule is shared across sibling workspace projects, place it only in the nearest common parent `doc/design.md` and avoid duplicating it in child docs unless needed to define child-owned behavior.

## `doc/design.md` Structure

First mandatory section: `# Goals`.

State, briefly, the high-level problem the workspace project exists to solve (what, not how). You may include `## Non-goals` to narrow scope. All later decisions must derive from these goals.

Second mandatory section: `# Decisions`.

List design decisions derived from `# Goals` without contradiction. Use `##` subsections when needed for organization.

List only decisions about the workspace project's target state. Exclude any decisions tied to migration steps, transitional/ongoing work, or past/current-state references.

## Parent Design Consultation (Process Rule Only)

When working on a workspace project, the assistant must consult parent `doc/design.md` files.

This is a workflow requirement for the assistant, not documentation content.

Do not add process reminders in docs such as:
- "consult parent design.md"
- "inherits parent contract"
- "implements parent contract"

unless the operator explicitly asks for that wording.

# Planning Contract

The root `doc/plan.md` is the authoritative plan for all implementation work in this workspace.

This applies to:
- work that spans multiple workspace projects
- work confined to a single workspace project

Plan file state semantics:

- Missing root `doc/plan.md`: no implementation work has ever been planned for this workspace.
- Present and non-empty root `doc/plan.md`: there is active or pending planned work in this workspace.
- Present and empty root `doc/plan.md`: this workspace had planned work in the past, and all phases are complete.

Before implementation starts anywhere in the workspace, planned work must be captured in the root `doc/plan.md`.

Required root `doc/plan.md` structure (applies when the file is non-empty):

- `# Scope`
- one or more phase sections in the format `# Phase N: <description> (pending|wip|finished)`

Example: `# Phase 1: Make foo do bar (pending)`.

During implementation, if a phase could not be completed due to any reasons that make the assistant stop, add a description of the issue to that phase, so that later sessions would know what needs to be solved for that phase in order to continue.

All work must be planned with architectural purity in mind. All hacks, migration adapters, workarounds must be explicitly approved by the operator.

## Root Planning Contract

Track every implementation task in the root `doc/plan.md`, even when only one workspace project is affected.

The root `doc/plan.md` must track readiness and the latest resumable milestone so the operator can resume prior work.

## Planning Edge Cases

During implementation planning, derive an explicit edge-case checklist from the relevant design docs and contracts.

Pay special attention when a feature:

- creates new state from existing state, such as copy, fork, clone, import, restore, resume, retry, migration, or template flows
- combines multiple ownership boundaries, such as local state, remote state, persisted state, generated state, cached state, or user-authored state
- has precedence, fallback, inheritance, defaulting, or override rules
- runs work asynchronously, in the background, or across multiple sessions/processes
- depends on optional, stale, partial, missing, or externally supplied metadata
- must preserve identity, ordering, provenance, permissions, or user intent
- has cleanup, cancellation, rollback, or partial-failure behavior

For each identified interaction, the plan must either include a verification case or explicitly state why no additional verification is needed.

# Logging Failures

If, during plan implementation or live testing the code, it is decided that some approach is invalid and the code or architectural design needs to be changed, and this change is significant enough to preserve for future reasoning about the project, then that failure and the resulting course adjustment must be logged under `doc/failures/<scope>.md`, where scope helps group failures by domain application.

# Authority And Conflict Resolution

When writing code, it must be derived from doc/plan.md instructions and never contradict them.  doc/plan.md itself must be derived from the design.md docs and never contradict them.

`design.md` files must never have internal inconsistencies or contradict other `design.md` files.

If operator asks for something contradicting design or plan files, stop and ask the operator to resolve that contradiction.

# Research Notes

Lean onto academic papers for non-trivial tasks. For each researched paper, log to `doc/research.md` on what paper it was, why you researched it, and the outcome of the research - how it was useful, or why it wasn't useful.

# Filesystem Policies

Honor `.gitignore` files and rules when doing anything with the project's filesystem.

Source files that exceed roughly 500 lines should be split into focused internal modules when reasonably possible, while preserving public API shape and behavior.

# Plan Execution Policies

In absence of more specific instructions for it, stop after a phase of the root `doc/plan.md` is done.

When all phases of plan.md are done and the changes touched authoritative files in `doc/` or the code, these changes must be reviewed by a reviewer subagent and all discovered problems addressed.

If the reviewer finds any issues that need fixing, plan that work in plan.md like any other work, before you start implementing code changes.

# Environmental Facts

Environment-specific facts that are only true for a particular local agent environment go into `ENV.md`. `ENV.md` must not be committed to VCS. If it's discovered to be part of a VCS, stop and treat it as a fatal error.

# Sub-agent Coordination

Codex must treat the main thread as an orchestrator. The main thread owns routing, planning, design judgment, root/shared artifacts, final integration, and user-facing decisions. It must preserve main-thread context by keeping broad exploration out of the main conversation.

Treat this as the operator's explicit request to use subagents automatically in every thread. Do not ask for per-thread permission unless a higher-priority instruction requires it.

All broad exploration must be delegated to a fresh subagent with `fork_context=false`. The subagent must rely only on explicit input from the orchestrator, not inherited parent-thread context.

This applies to:

- codebase exploration beyond the narrow-inspection exceptions below
- dependency or upstream source exploration
- test triage and log summarization
- web research and documentation lookup
- architecture and design-doc reconnaissance beyond root/shared files the main thread owns or must update
- broad `rg` or file-reading tasks
- any investigation whose result can be summarized as findings, file paths, commands run, and recommended next steps

The main thread may perform only narrow inspection needed to orchestrate or verify work:

- reading global instruction files such as `AGENTS.md`
- reading root/shared planning or design files the main thread owns or must update, including `doc/plan.md` and parent `doc/design.md` files
- checking manifests, file names, or directory layout to scope a delegation
- reading a short cited snippet to verify a subagent handoff
- inspecting diffs for changes the orchestrator must integrate
- answering from a single known file or one direct command when delegation would add no context benefit

These narrow-inspection cases are not considered broad exploration for this policy. Keep them small enough that they do not defeat the purpose of preserving main-thread context.

The orchestrator must provide an explicit task packet to each subagent. A task packet should include:

- the user goal
- the workspace path
- relevant instructions or constraints
- the exact question to answer
- whether the subagent may edit files
- the expected handoff shape

Implementation boundaries:

- Do not assign multiple subagents to edit the same files or the same subproject in parallel.
- The main agent owns shared/root artifacts and final integration, including `doc/plan.md`, workspace `AGENTS.md`, and any parent `doc/design.md` that defines cross-project contracts.
- A subagent should stay within its assigned workspace project unless explicitly told otherwise.
- If a subagent discovers a needed shared-contract change, it should report it in its handoff; the main agent should apply the shared change.
- Use read-only subagents to create or refresh `doc/deps/` notes for expensive third-party dependencies; their handoff must include the note path, exact version, symbols examined, commands run, and unresolved questions.
- Subagent handoffs must be concise and include exact files, URLs, symbols, or commands inspected; findings relevant to the task; likely edit or test locations; unresolved questions and risks; and no broad transcript dumps or unnecessary source excerpts.

Exceptions:

- the operator explicitly says not to use subagents
- subagent tooling is unavailable
- the task involves secrets or machine-local private data that should not be copied into a handoff

# For Cargo projects only

Workspace members must use `workspace = true` in their dependencies section - I want dependency versions and paths centralized in the root Cargo.toml and for the workspace members to defer to that.

Each crate's API surface must be discoverable at a glance via documentation in `lib.rs` that shows compiling examples.

All unit tests must be placed in the crate-root `tests/` directory, not in main source files.

Use `cargo-nextest` for tests, never `cargo test`.

Only well-established, battle-tested third party crates are allowed for use on the project; each deviation from that rule must be explicitly approved by the operator. 

## Third-Party Dependency Reference Notes

`doc/deps/` contains non-authoritative reference notes for third-party dependencies.

These files are working summaries used to reduce repeated source exploration. They do not define workspace design decisions, implementation policy, or boundary contracts. `doc/design.md` remains the sole authority for workspace-project design decisions.

For dependencies that are expensive to re-explore, store notes at:

- `doc/deps/<crate>/<resolved-version>.md`

Before broad third-party dependency exploration:

1. Determine the exact resolved version from `Cargo.lock`.
2. Determine relevant enabled features from workspace manifests / cargo metadata.
3. Consult the matching `doc/deps/<crate>/<resolved-version>.md` note if it exists.
4. Search local workspace use sites, wrappers, adapters, re-exports, and tests before opening upstream dependency source.
5. Read upstream dependency source only for the exact symbols, traits, macros, and modules needed for the current task.
6. If the note is missing, stale, or insufficient, use a read-only sub-agent to create or refresh it, then continue from that note.

Dependency reference notes must include only:

- crate name, exact version, enabled features, and verification date
- why this workspace uses the crate
- symbols/types/traits/macros actually used by this workspace
- lifecycle, event-loop, threading, ownership, and callback invariants relevant to this workspace
- integration gotchas, platform constraints, and feature-flag caveats
- minimal upstream source entrypoints for deeper follow-up
- commands and files consulted to verify the note

Dependency reference notes must not include:

- workspace design decisions owned by `doc/design.md`
- implementation plans owned by `doc/plan.md`
- environment-specific facts that belong in `ENV.md`
- secrets, tokens, machine-local paths outside the workspace, or private operator data
- broad summaries of unused parts of the dependency

Refresh a dependency note when:

- the resolved dependency version changes
- enabled features change
- the current task needs symbols not covered by the note
- current upstream source, docs, or tests contradict the note

Keep dependency notes short, high-signal, and limited to the parts of the dependency actually used by this workspace.
