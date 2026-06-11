# Reason For Investigation

Beryl targets exactly one Codex App Server contract per release. The CAS 0.137 migration needed a compact evidence note for the current target contract without carrying historical observations from older app-server versions.

# Outcome

Useful. For this Beryl version, the target runtime is `codex-cli 0.137.0` / Codex App Server 0.137.0. Compatibility must be proven by parsing the app-server version from `initialize.userAgent` and by required-method probes, not by assuming that a newer app-server version enables a different protocol path.

The current 0.137 transcript-history contract is `thread/turns/list` with `itemsView`. The current 0.137 permission contract exposes sandbox controls at thread, fork, and turn boundaries. The current 0.137 thread schema exposes direct subagent hierarchy through nullable `Thread.parentThreadId`.

These observations support the root design invariant but do not replace it. `doc/design.md` remains the design authority.

# Sources

- Legacy note: `doc/app-server-contract.md`, migrated on 2026-06-11.
- Local codex-cli 0.137.0 generated stable app-server schema from `codex app-server generate-json-schema --out <temp-dir>`, accessed 2026-06-11.
- Local codex-cli 0.137.0 generated experimental app-server schema from `codex app-server generate-json-schema --experimental --out <temp-dir>`, accessed 2026-06-11.
- Local CAS 0.137 live probes listed in sibling memory notes under `doc/memory/topic/codex-app-server/`.

# Notes

- Beryl does not bundle or install Codex. These observations describe the configured app-server runtime that Beryl launches and probes.
- Beryl should treat schema presence as insufficient proof of usable runtime behavior. Runtime probing remains required for required methods and known edge cases such as schema-exposed methods that return unsupported at runtime.
