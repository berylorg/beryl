# App-server Initialize User Agent

## Executable banner equality

Phase 4 planned to validate the exact `codex-cli 0.146.0` executable twice: through a bounded `--version` process and by requiring initialize response `userAgent` to equal the same CLI banner.

The first live probe disproved the second assumption. The exact executable returned `codex-cli 0.146.0` from `--version`, but initialize returned a composed value shaped as `beryl/0.146.0 ... (beryl; 0.1.0)`. The field incorporates Beryl's initialize `clientInfo`; it is not the CLI banner and cannot satisfy exact banner equality for a Beryl client.

The probe stopped at that assertion before it created a root thread or started model work. Its cleanup guard shut down and reaped the task-owned app-server and removed its managed authentication material. The existing Beryl app-server process remained the only `codex` process.

The exact absolute executable path plus bounded `--version` check remains valid target evidence. Operator confirmed that CAS reports its version by combining the client project's name with the CAS version. The accepted correction is to require the `beryl/0.146.0` prefix from initialize while retaining the separate exact executable and CLI-banner checks. No dynamic-tool retention conclusion can be drawn from the stopped pre-root run.

Affected authority and evidence:

- `doc/plan.md`, Phase 4.
- `doc/app-server-contract.md`, initialize response semantics.
- `doc/memory/topic/codex-app-server-0.146.0/forked-dynamic-tool-retention.md`.
- `crates/beryl-backend/tests/live_dynamic_tool_fork.rs`.
- Live command: `cargo nextest run -p beryl-backend --test live_dynamic_tool_fork --run-ignored ignored-only` with the two documented opt-in environment variables set process-locally.
