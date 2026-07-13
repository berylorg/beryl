# Reason For Investigation

The Beryl-home rework needs one existing Codex App Server field that can carry recovered Syndic history and branch-discussion context separately from ordinary user input and developer instructions, without modifying Codex App Server.

# Outcome

Useful. The locally installed `codex-cli 0.144.1` experimental V2 schema exposes nullable `additionalContext` on both `TurnStartParams` and `TurnSteerParams`.

`additionalContext` is an object keyed by an opaque client-selected source identifier. Each value is an `AdditionalContextEntry` with required string `value` and required `kind`. `AdditionalContextKind` accepts exactly `untrusted` or `application`.

The schema establishes a normalized protocol shape suitable for a Beryl-owned boundary. It does not specify size limits, truncation, prompt ordering, persistence across later CAS turns, or whether a given Beryl payload should be classified as `application` or `untrusted`; Beryl's target system design must define those rules and verify runtime behavior before relying on them.

Follow-up exact-source and live evidence is recorded in `additional-context-runtime-0.144.1.md`. It found a silent per-value truncation limit above approximately 4,000 bytes plus replay-state limits that make the current single-large-entry materialization proposal nonviable.

This evidence supports changing Beryl's single targeted CAS contract from 0.137.0 to 0.144.1. It does not itself define that project decision.

# Sources

- Local executable: `codex.exe`, reporting `codex-cli 0.144.1`, inspected 2026-07-10.
- Generated experimental schema: `codex app-server generate-json-schema --experimental --out <temporary-directory>`, inspected 2026-07-10.
- Generated schema definitions: `v2/TurnStartParams.json`, `v2/TurnSteerParams.json`, `AdditionalContextEntry`, and `AdditionalContextKind`.

# Commands

```text
codex.exe --version
codex.exe app-server generate-json-schema --experimental --out <temporary-directory>
```

# Refresh Triggers

- Refresh under a new sibling file when Beryl targets another Codex App Server version.
- Refresh if runtime probes contradict the generated 0.144.1 schema or expose size, ordering, persistence, or trust semantics needed by Beryl.
