# Scope

Initial `rag-rat-project-docs` onboarding with rag-rat 0.23.0 and a repository-local index.

# Invalidated approach

Install `BAAI/bge-small-en-v1.5` before creating the repository index, then treat every embedding
reported as `missing` by `doctor` as unfinished reconciliation work.

# Evidence

- `rag-rat --json models install BAAI/bge-small-en-v1.5` failed before schema initialization with
  `no index at this path yet`.
- `rag-rat --json index --discover` initialized the schema and reported the configured model as
  `MissingModel`; the same install command then succeeded and reported `Ready`.
- The first foreground `reconcile --changed-first --until-clean` embedded 2,466 chunks, classified
  298 as `SkipTooSmall`, reported zero failed and blocked chunks, and returned `Current`.
- A second complete discovery and foreground reconciliation processed zero chunks, classified the
  same 298 as `SkipTooSmall`, again reported zero failed and blocked chunks, and returned `Current`,
  while `doctor` continued to expose those policy-skipped chunks through its `missing` counters.

# Why it failed

Rag-rat 0.23.0 requires an initialized index schema before its explicit model installer can run.
Its doctor counters also do not distinguish `SkipTooSmall` policy exclusions from actionable
missing embeddings, so the raw `missing` value is not by itself a semantic-backlog test.

# Course correction

For a fresh 0.23.0 project database, initialize discovery first, install the configured model,
then run the required foreground discovery and reconciliation barrier. Accept freshness only when
reconciliation returns `Current` with zero failed and blocked chunks; preserve the explicit
`SkipTooSmall` evidence when doctor still counts those chunks as missing. Continue to require a
ready installed model, fresh FTS, zero unindexed files, and a focused hybrid semantic query.

# Affected authority

`doc/plan.md` Phase 229 and the local `rag-rat.toml` project-doc target.

# Remaining risk

A later rag-rat release may change bootstrap ordering or doctor accounting. Requalify these checks
before changing the pinned 0.23.0 integration.
