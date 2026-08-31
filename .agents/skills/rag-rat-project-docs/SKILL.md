---
name: rag-rat-project-docs
description: Use optional rag-rat semantic retrieval for project Markdown, especially AIPM-prescribed design, plan, rework, GUI, memory, failure, world, and package documents. Use when this skill is installed and work may discover, read, create, edit, move, or delete those documents; set up the pinned Node-free MCP integration when needed, search semantically before broad manual navigation, and block on a fully current index after AI-authored Markdown changes.
---

# rag-rat Project Docs

## Core Contract

Treat the filesystem as authority and the rag-rat index as derived navigation state. Semantic
retrieval narrows what to read; it never overrides an authoritative document, proves completeness,
or replaces a direct read before editing or relying on exact wording.

This skill covers every Markdown file selected by the project's rag-rat Markdown target. That
target must include active and supporting AIPM documents, including linked supplements, project GUI
docs, plans, rework trackers, exploration memory, failure records, and project-declared authorities.
Archived obsolete rework documents may be deliberately excluded so they do not rank as live
authority.

Once per session, confirm the available `rag-rat` reports exactly version `0.23.0`. First use shell
command discovery; do not invoke a missing executable. Do not silently accept another version.

If the executable, model, `rag-rat.toml`, initial index, or project-scoped Codex MCP configuration
is absent or unhealthy, read [setup and recovery](references/setup-and-recovery.md). Never use
Node, npm, npx, or a package that transitively requires them for this integration.

## Retrieval Workflow

- Read a known authority entry point directly when its exact path is already established.
- Otherwise call `index_status`, then use `semantic_search` with a focused question before broad
  `rg`, directory traversal, or opening several candidate documents.
- Use one or two refined searches when the first result set is weak. Use `read_chunk` only after a
  search has identified a relevant chunk, then directly read the owning file for authoritative
  context and exact language.
- Use exact filesystem listing, `rg`, and direct reads for authority-chain enumeration,
  contradiction checks, exhaustive coverage, exact identifiers, and proof that something is
  absent. Semantic retrieval is not a completeness oracle.
- Do not issue synthetic known-answer semantic queries as a routine session, post-write, or
  authority-decomposition gate. After initial setup or a major document reorganization, one focused
  known-answer spot check is optional only when concrete uncertainty remains about semantic
  retrieval quality; directly read the returned source to confirm provenance. Use structural links,
  routing, exact inspection, and direct authoritative reads to validate decomposed authority.
- If MCP search is unavailable, unhealthy, or returns `needs_reindex`, attempt the documented
  recovery. Fall back to manual navigation only after making the degraded mode explicit; never
  represent manual fallback as a successful semantic search.

## Blocking Post-Write Barrier

Immediately after an AI filesystem operation creates, changes, renames, moves, or deletes any
indexed Markdown file, block before subsequent project work, retrieval, handoff, commit, or success
claim. From the repository root, run these foreground commands in order:

```text
rag-rat --json index --discover
rag-rat --json reconcile --changed-first --until-clean
```

Do not add `--max-seconds`. Require both commands to finish. Parse the reconcile response and accept
only `status: "Current"` with zero failed or blocked chunks. A zero process exit code is not enough:
`Blocked`, `Failed`, and `Partial` responses are freshness failures.

The barrier applies once to all Markdown paths changed by one atomic filesystem operation. If a
later operation changes another indexed Markdown file, run it again. It covers additions and
deletions through discovery as well as embedding convergence.

Do not substitute any of these weaker mechanisms:

- the background watcher, because it is asynchronous;
- `maintenance`, because it is time-budgeted and may coalesce;
- MCP `heal_index`, because it does not discover brand-new files;
- search-time auto-healing, because it is bounded and query-driven;
- process exit status without inspecting structured reconciliation status.

If the barrier fails, preserve the authoritative Markdown change, stop dependent work, report that
the derived index is stale and include the failing status or diagnostic. Do not silently continue
with manual navigation, repeatedly retry, alter the embedding model, rebuild unrelated state, or
upgrade rag-rat without Operator direction.

## Concurrency And Scope

Let rag-rat's per-index writer lock serialize agents. Do not bypass its lock, point concurrent
writers at copied databases, or launch an extra long-lived watcher. The strict multi-process
contract is unsupported on NFS and WSL2 `/mnt` filesystems where rag-rat documents file locks as
unreliable; stop setup there and ask the Operator to use a native filesystem.

Do not index secrets, generated dependencies, installed skills, Codex configuration, rag-rat state,
vendored trees, or archived obsolete rework documents. Preserve any broader pre-existing rag-rat
configuration rather than replacing it; add the project Markdown target without removing source or
other targets the Operator already uses.
