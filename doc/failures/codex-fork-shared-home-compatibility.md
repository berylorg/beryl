# Codex fork shared-home compatibility

## Scope

Live verification of Beryl's exact standalone Host-Windows app-server override against the maintained `../codex-fork/codex-rs/**` artifact and the active shared Codex home.

## Invalidated assumption

Moving Beryl usage accounting out of upstream `state_5.sqlite` was necessary, but it was not sufficient to make a locally built fork compatible with the existing shared home. SQLx migration checksums also depend on the exact line-ending bytes materialized when the executable is built.

## Decisive evidence

- The original LF-built `dde97dce2f` standalone artifact, SHA-256 `AC58227F1A0546B7A413CFE120FE4A1A1B8B9FD213CF9A39A0C1A01D1E8252BE`, passed 40 focused fork tests. Beryl formatting and 192 focused tests also passed.
- Installed vanilla `codex-cli 0.153.2` bound successfully against `C:\Users\user\.codex`; that fork artifact exited before binding with `failed to initialize sqlite state runtime under C:\Users\user\.codex`.
- A read-only `_sqlx_migrations` probe found matching successful version and description sets but checksum mismatches for state versions 1 through 52, log, goal, and queue versions 1 through 2, and memory version 1.
- For a representative migration from each fatal database, the applied checksum exactly matched an in-memory LF-to-CRLF transformation of the current SQL. The compiled checksum matched the LF working-tree file and raw Git blob.
- For `state/migrations/0001_threads.sql`, the applied CRLF checksum is `54BBD6F47905A4E4C674034575963D82DA7B534E66E9A37A81EC2AFB6A4B56CE6DE9B3ECF3032796A800F650239847D4`; the original LF checksum is `627EF19164C9BB298A0CD99945981C9B7BDA3D9E6CF12EB35145E3B1D3BF7CF8740F0DBAA0B475185FC2993397078049`.
- At diagnosis, the checkout had `core.autocrlf=false`, its representative file had 25 LF and zero CRLF sequences, and no effective scoped attribute forced migration SQL line endings.

## Why the approach failed

Initialization runs the fatal upstream database migrators before opening the optional Beryl accounting store. SQLx permits applied versions absent from the executable when `ignore_missing` is enabled, but it still rejects a known version whose stored checksum differs from the compiled checksum. The shared databases were migrated by a CRLF-materialized build, while the original fork artifact embedded LF migration bytes, so all five fatal database checks failed deterministically before Phase 8's accounting cutover could run or degrade gracefully.

## Accepted source correction

The accepted correction uses `codex-rs/state/.gitattributes` with `text eol=crlf` for upstream migration SQL. The original five-directory correction covered initialization but omitted `thread_history_migrations`, which is opened on thread-history access. The correction now includes that directory and materializes all six of its migrations as CRLF. Git-normalized SQL contents, accounting migrations, SQLx validation, and shared-home ledgers remain unchanged. This is a v0.1 Host-Windows build-compatibility fix, not general cross-platform checksum reconciliation.

On 2026-09-05, a freshly compiled, temporary `codex-state` checksum probe passed against all 59 applied migration records across the five shared-home databases. Read-only connections compared version, description, success, and checksum to the production compiled migrators, including the published state version 1 CRLF checksum above. The focused `init_records_successful_sqlite_init_phases_to_explicit_telemetry` runtime regression also passed with cargo-nextest. Effective-attribute, working-tree byte, and normalized-diff checks passed; the temporary probe was removed after verification.

## Executable acceptance

Phase 16 passed on 2026-09-05. The freshly built standalone executable initialized through Beryl's exact Host-Windows override against `C:\Users\user\.codex` and recognized `thread/tokenUsageTree/read` for an ephemeral no-turn root. `CODEX_HOME` was unset; the smoke asserted the default home against the initialize response. The observed response was the exact supported `-32600` message `usage tree not found for thread <root-id>`, not an unsupported-method response. This qualifies initialization and method recognition; it does not exercise a model turn or the snapshot branch.

- Source: `dde97dce2f26da7c7abe0c20cfd54822297eb719` plus the Phase 15 working-tree correction. The scoped `.gitattributes` SHA-256 is `14D785C785F0D4618D9EDF4DD9BF8B2254E55CD4094F35D3394B8D18FEB66055`; all 59 covered SQL files were CRLF, with empty normalized SQL and accounting diffs.
- Retained Beryl-relative artifact: `target/codex-app-server-fork/dde97dce2f-phase16-crlf/codex-app-server.exe`, 227,150,848 bytes, SHA-256 `FC883F9D04BB1709357BA2D2CA6899FEFA1A578BC1FCF4FC801E0145F35B029D`.
- From the Beryl workspace, `cargo build --manifest-path ../codex-fork/codex-rs/Cargo.toml --locked --release -p codex-app-server --bin codex-app-server --target-dir C:\Users\user\p\berylorg\beryl-v0.1\.codex-tmp\phase16-dde97dce2f-migration-crlf --jobs 1` passed with Rust/Cargo 1.97.1. Existing unused-code and `proc-macro-error2` future-incompatibility warnings were nonblocking.
- With process-local `BERYL_RUN_LIVE_FORK_EXTENSIONS_TEST=1` and `BERYL_LIVE_STANDALONE_APP_SERVER_EXECUTABLE` set to that exact absolute artifact path, `cargo nextest run --locked -p beryl-backend --test live_fork_extensions --run-ignored only standalone_fork_usage_tree_smoke_uses_beryl_session_boundary --success-output immediate` passed. Cargo-nextest was 0.9.129.
- The companion `unused_ephemeral_root_recognizes_only_exact_no_usage_tree_response` passed with `--run-ignored default`. The smoke's stale message expectation was corrected to require the exact root ID, method, and error code; wrong-root, unsupported-method, malformed-suffix, unrelated-error, and timeout cases remain rejected. Formatting, worker self-review, and main-thread source, artifact-digest, and scoped-diff review passed.

The task-owned build cache was removed with Cargo clean (9,825 files, 7.4 GiB); the exact cache path is absent. Managed shutdown passed and no app-server process remained. Documentation verification used the filesystem under the existing no-install semantic-index exception.

## Thread reopening regression evidence

On 2026-09-05, `thread/resume` exposed the omitted thread-history directory with `migration 1 was previously applied but has been modified`. The new crate-root `codex-state` integration target `thread_history_migration_checksums` seeds all six established CRLF migrations into a disposable database and reopens it through `open_thread_history_db`. The positive test reproduced the exact error with the original LF files and passed after correction. Its companion test confirms an unrelated checksum mismatch remains rejected without changing the applied ledger. Focused nextest passed 2/2; all six checkout attributes and CRLF byte representations were verified. No private database was read or modified for this regression.

Beryl independently misclassified every resume error as requiring rebinding before it had any backend record to validate. Those failures now report activation failure with the backend diagnostic. They never persisted a rebind requirement. The 11 `beryl-app` thread-activation integration tests passed, including exact-thread retry after failed resume and continued rejection of actual working-directory mismatch.

Both development executables built successfully: Beryl `target/debug/beryl.exe` and fork `../codex-fork/codex-rs/target/debug/codex-app-server.exe`. The latter's SHA-256 is `8DECC8EFB0CEB265B2F4DD712E8FC4FD17082AFAEF15D87FAEBB217D7B060A8F`. Both command-line help smoke checks passed. The installed binaries and running processes were left unchanged; reopening the Operator's actual thread still requires restarting with these corrected builds.

## Known cleanup residue

Phase 15's disposable `../codex-fork/codex-rs/state/.phase15_rematerialize.exe` (197,120 bytes) and `.phase15_rematerialize.pdb` (1,454,080 bytes) remain. Automatic approval review rejected exact-file deletion with only a generic blocked-policy reason. Phase 16 left these inert files untouched; no task process uses them.

The user-visible target design and separate usage-accounting storage design remain unchanged. All phases of the root implementation plan are complete.
