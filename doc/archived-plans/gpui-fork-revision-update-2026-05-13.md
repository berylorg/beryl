# Scope

Switch Beryl's GPUI dependency to the pushed `gpui-fork` GitHub revision that contains the renderer image atlas cleanup fix.

Relevant design constraints:

- Beryl may depend on a Beryl-maintained GPUI fork when targeted patches are needed to satisfy Beryl's product constraints.
- Beryl's normal dependency on the GPUI fork must preserve GPUI's public boundary and keep the default HTTP client stack gated behind the fork's opt-in feature.
- RAM efficiency is a first-order design constraint, and the completed GPUI fork fix releases renderer image resources when completed image assets are removed.

Edge-case checklist:

- Preserve the existing GitHub dependency source and only advance the revision to the pushed commit containing the GPUI fork fix.
- Update `Cargo.lock` so the resolved source matches the manifest revision.
- Do not disturb unrelated dirty Beryl implementation changes from the completed memory investigation.
- Verify `ENV.md` remains untracked.

# Phase 1: Update Beryl to the pushed GPUI fork revision (finished)

Work items:

- Identify the current Beryl GPUI fork dependency declaration and lockfile resolution.
- Resolve the full pushed GPUI fork commit hash from the local `zed-fork` branch.
- Update the Beryl dependency revision and lockfile to that commit.
- Run focused verification that Cargo resolves the new revision and that the workspace still checks.

Verification cases:

- Manifest and lockfile agree on the new GPUI fork revision.
- Cargo verification passes without changing unrelated dependency versions unexpectedly.
- `git diff --check` passes.
- `ENV.md` is not tracked.

Progress on 2026-05-13:

- Updated Beryl's direct `gpui` workspace dependency in root `Cargo.toml` from `f2193db331be6424be223f7ea9982c06b978a16a` to pushed zed-fork commit `c70dbf5244396cea7e627a73ffdcf45f19c9642d`.
- Confirmed local `zed-fork` `gpui-fork` HEAD and remote `origin/gpui-fork` both point to `c70dbf5244396cea7e627a73ffdcf45f19c9642d`.
- Confirmed the local `.cargo/config.toml` path patch was restored after each clean-resolution attempt.
- With local path patches enabled, `cargo check --workspace --all-targets` passes with the existing warning set.

Current blocker:

- With local `.cargo/config.toml` disabled, Cargo resolves Beryl's direct `gpui` dependency to `c70dbf5244396cea7e627a73ffdcf45f19c9642d`, but the published `gpui-text-input` commit `fd4f2caf8c39981f7b829f276c7ade48430eca83` and `gpui-settings-window` commit `c8b4e002f283b444f2286b92ce283c711a586ce0` still depend on zed-fork commit `f2193db331be6424be223f7ea9982c06b978a16a`.
- That creates two different `gpui` crates in a clean GitHub-resolution build and `cargo check --workspace --all-targets` fails with `FocusHandle`, `Pixels`, `Window`, `Context`, `Render`, and `AppContext` type mismatches between the two GPUI revisions.
- A tracked Cargo `[patch."https://github.com/berylorg/zed-fork.git"]` entry pointing back to the same public repository was attempted and rejected by Cargo because patches must point to different sources.

Needed to continue:

- Preferred clean fix: update and push `gpui-text-input` and `gpui-settings-window` so their workspace `gpui` dependency also points to zed-fork commit `c70dbf5244396cea7e627a73ffdcf45f19c9642d`, then update Beryl's pins for those two crates and regenerate `Cargo.lock` without relying on the local path patch.
- Alternative requires explicit approval: use a Cargo source-override workaround in Beryl to force transitive old zed-fork users to the new GPUI revision.

Additional progress on 2026-05-13:

- Updated `gpui-text-input` locally to zed-fork commit `c70dbf5244396cea7e627a73ffdcf45f19c9642d`.
- Verified `gpui-text-input` with local Cargo path patch disabled using `cargo check --workspace --all-targets`.
- Committed `gpui-text-input` locally as `c076415e41804812cb031fbe41e8bffcfc8e0a3e` with commit title `Update GPUI fork revision`.
- Began updating `gpui-settings-window` to zed-fork commit `c70dbf5244396cea7e627a73ffdcf45f19c9642d` and `gpui-text-input` commit `c076415e41804812cb031fbe41e8bffcfc8e0a3e`, but Cargo cannot fetch that text-input revision until it is pushed to `https://github.com/berylorg/gpui-text-input.git`.

Resolved blocker:

- `gpui-text-input` commit `c076415e41804812cb031fbe41e8bffcfc8e0a3e` was pushed to GitHub by the operator.

Additional progress after `gpui-text-input` push:

- Verified `gpui-settings-window` with local Cargo path patches disabled using `cargo check --workspace --all-targets`; Cargo resolved zed-fork commit `c70dbf5244396cea7e627a73ffdcf45f19c9642d` and `gpui-text-input` commit `c076415e41804812cb031fbe41e8bffcfc8e0a3e` from GitHub.
- Committed `gpui-settings-window` locally as `c6973c27dd0f23259228914d680a313f665b3f36` with commit title `Update GPUI dependency revisions`.
- Updated Beryl root `Cargo.toml` to pin `gpui` to `c70dbf5244396cea7e627a73ffdcf45f19c9642d`, `gpui-text-input` to `c076415e41804812cb031fbe41e8bffcfc8e0a3e`, and `gpui-settings-window` to `c6973c27dd0f23259228914d680a313f665b3f36`.
- With Beryl local Cargo path patches enabled, `cargo check --workspace --all-targets` passes with the existing warning set.

Current immediate blocker:

- Push `gpui-settings-window` commit `c6973c27dd0f23259228914d680a313f665b3f36` to GitHub, or explicitly authorize this agent to push it. After that, Beryl can be verified with local path patches disabled and `Cargo.lock` can be regenerated for the clean GitHub-resolution dependency graph.

Completed on 2026-05-13:

- The operator pushed `gpui-settings-window` commit `c6973c27dd0f23259228914d680a313f665b3f36`; remote `origin/main` now points at that commit.
- Regenerated Beryl `Cargo.lock` with local `.cargo/config.toml` path patches temporarily disabled, then restored the local Cargo config.
- Verified clean GitHub dependency resolution with `cargo check --workspace --all-targets`. Cargo resolved `gpui` and all zed-fork helper crates from zed-fork commit `c70dbf5244396cea7e627a73ffdcf45f19c9642d`, `gpui-text-input` from `c076415e41804812cb031fbe41e8bffcfc8e0a3e`, and `gpui-settings-window` from `c6973c27dd0f23259228914d680a313f665b3f36`.
- `Cargo.toml` and `Cargo.lock` now agree on the new GitHub dependency revisions.
