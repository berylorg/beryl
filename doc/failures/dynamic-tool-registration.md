# Dynamic Tool Registration

## Codex app-server 0.146 tagged specification

Phase 1 proposed proving forked-child dynamic-tool retention through Beryl's existing `DynamicToolSpec` and managed backend launch path.

That approach is invalid for the installed `codex-cli 0.146.0` contract. Generated experimental schema requires tagged dynamic-tool specifications with `type = "function"` or `type = "namespace"`, while Beryl serializes the older flat specification and its protocol tests assert that older wire shape. App-server exposes no post-fork or post-resume tool-registration request.

The live retention probe compiled but was not run because a rejected or bypassed root registration would not prove the production contract. The course correction is to stop Phase 1 and request Operator direction on a separate backend compatibility phase; raw-protocol registration and unrelated fresh threads are not accepted substitutes.

Operator subsequently authorized that compatibility phase. Beryl now keeps its logical flat function registry but normalizes it through a private tagged `thread/start` wire representation. The original direct-serialization approach remains invalid and must not be restored.

Affected authority and evidence:

- `doc/plan.md`, Phase 1.
- `doc/app-server-contract.md`.
- `doc/memory/topic/codex-app-server-0.146.0/forked-dynamic-tool-retention.md`.
- `crates/beryl-backend/tests/live_dynamic_tool_fork.rs`.

The registration prerequisite and retention gate are now satisfied. A bounded live probe against exact 0.146.0 proved the correctly registered tagged namespaced tool survives fork, full rollback, archive, unarchive, resume, and a later child turn. The original flat-wire registration remains invalid and must not be restored.
