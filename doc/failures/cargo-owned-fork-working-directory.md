# Owned-Fork Cargo Verification Must Start At The Fork Root

## Scope

This applies when Beryl verifies an owned Cargo workspace or nested owned crate outside the Beryl
workspace, including `..\fjall-fork` and its nested `lsm-tree` workspace.

## Invalidated Approach

Phase 48 initially invoked Fjall with `--manifest-path` while the command's working directory
remained the Beryl root. Cargo inherited Beryl's `.cargo/config.toml` patches and reported that
Fjall's ignored `Cargo.lock` needed an update under `--locked`. Treating that result as an actual
Fjall lock mismatch created a false implementation blocker.

## Decisive Evidence

The exact selected Fjall and `lsm-tree` suites pass under `--locked --offline` when each command
starts from its owning workspace root. They cover 17 and 23 tests respectively. Fjall's ignored
lockfile remained 21,402 bytes with modification time `2026-07-28 00:26:33` and SHA-256
`8507E54766572EA8573E640B3116D8BF1DEBE56EDC39D29E5617569A649E1670`; the reconciliation attempt
did not rewrite it or change any third-party version.

## Course Correction

Run Cargo for an owned external workspace with that workspace as the process working directory.
Run a nested independently configured workspace from its own root. Keep `--locked --offline` for
verification, and first repeat any apparent lock mismatch from the owning root before authorizing a
lockfile change. `--manifest-path` selects a manifest; it is not a substitute for the intended
Cargo-configuration discovery root.

