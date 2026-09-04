# Codex fork shared-home compatibility

## Scope

Live verification of Beryl's exact standalone Host-Windows app-server override against the maintained `../codex-fork/codex-rs/**` artifact and the active shared Codex home.

## Invalidated assumption

Moving Beryl usage accounting out of upstream `state_5.sqlite` was necessary, but it was not sufficient to make a locally built fork compatible with the existing shared home. SQLx migration checksums also depend on the exact line-ending bytes materialized when the executable is built.

## Decisive evidence

- The current-HEAD `dde97dce2f` standalone artifact, SHA-256 `AC58227F1A0546B7A413CFE120FE4A1A1B8B9FD213CF9A39A0C1A01D1E8252BE`, passed 40 focused fork tests. Beryl formatting and 192 focused tests also passed.
- Installed vanilla `codex-cli 0.153.2` bound successfully against `C:\Users\user\.codex`; the current fork exited before binding with `failed to initialize sqlite state runtime under C:\Users\user\.codex`.
- A read-only `_sqlx_migrations` probe found matching successful version and description sets but checksum mismatches for state versions 1 through 52, log, goal, and queue versions 1 through 2, and memory version 1.
- For a representative migration from each fatal database, the applied checksum exactly matched an in-memory LF-to-CRLF transformation of the current SQL. The compiled checksum matched the LF working-tree file and raw Git blob.
- For `state/migrations/0001_threads.sql`, the applied CRLF checksum is `54BBD6F47905A4E4C674034575963D82DA7B534E66E9A37A81EC2AFB6A4B56CE6DE9B3ECF3032796A800F650239847D4`; the current LF checksum is `627EF19164C9BB298A0CD99945981C9B7BDA3D9E6CF12EB35145E3B1D3BF7CF8740F0DBAA0B475185FC2993397078049`.
- The current checkout has `core.autocrlf=false`, its representative file has 25 LF and zero CRLF sequences, and no effective scoped attribute forces migration SQL line endings.

## Why the approach failed

Current initialization runs the fatal upstream database migrators before opening the optional Beryl accounting store. SQLx permits applied versions absent from the executable when `ignore_missing` is enabled, but it still rejects a known version whose stored checksum differs from the compiled checksum. The shared databases were migrated by a CRLF-materialized build, while the current fork embeds LF migration bytes, so all five fatal database checks fail deterministically before Phase 8's accounting cutover can run or degrade gracefully.

## Required course correction

Repair the build or migration-validation boundary so unchanged upstream migrations remain compatible across the accepted Windows line-ending representations. The correction must recognize only byte-equivalent newline variants or otherwise produce the exact established Windows migration bytes; it must not rewrite the shared migration ledger blindly, suppress arbitrary checksum mismatches, or move fork state back into upstream databases. Select and authorize that narrow implementation before rebuilding and repeating the no-model-turn shared-home Beryl smoke.

The user-visible target design and separate usage-accounting storage design remain unchanged. Root [plan](../plan.md) owns repair sequencing and resumed verification.
