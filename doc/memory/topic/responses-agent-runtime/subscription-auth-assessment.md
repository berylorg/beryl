# Reason For Investigation

The CAS replacement needs a practical personal-Pro authentication boundary, including refresh,
account selection and failure behavior. Operator accepts OpenCode as precedent and authorizes
small direct subscription probes; production implementation remains blocked.

# Outcome

Independent direct inference and usage reads succeeded using the existing ChatGPT access token.
The live usage response reported Pro and matched the selected account. Browser/device login,
refresh and persistence mechanisms exist in pinned OpenCode/Codex source; CAS is not technically
required as an auth helper. See [direct access](direct-subscription-baseline.md),
[live account observations](subscription-account-observations.md) and
[OpenCode evidence](../../github.com/anomalyco/opencode/commit/5a8335857b0ebec44ef6aa1d52b339cf25c329ca/chatgpt-subscription-auth.md).

The key architecture choice is credential ownership across processes/homes, including refresh
rotation and account-switch/logout races. Source evidence does not prove safe independent writers
sharing one rotating credential family. No refresh, revocation, logout, plan change or concurrent
login was performed against Operator's account.

## Live Evidence Versus Source Evidence

Observed directly: current access worked; usage reported `pro` and the selected account; the
stored ID token was expired while the access token was not. Decoded local claims are not
signature-verified live entitlement proof. Plan availability on one account does not establish
the full personal-versus-managed selection policy.

Source-derived at official Codex commit
`6b9826e3aa83b1a5947db50f4332cb9c65f1b340`:

- `login/src/auth/manager.rs::AuthManager::should_refresh_proactively` checks access-token
  expiry within five minutes, falling back to an eight-day last-refresh interval when expiry
  cannot be parsed. It does not use stored ID-token expiry as the access gate.
- `AuthManager::auth` can retain cached auth after proactive-refresh failure.
  This suggests separating offline/transient failure from proven revoked credentials.
- `AuthManager::refresh_token` uses a per-manager semaphore and same-account reload, skipping
  refresh if stored credentials changed. This is useful coordination inside that manager,
  not a cross-process refresh transaction.
- `persist_tokens` reloads storage, updates fields present in the refresh response and saves,
  without an account/generation compare-and-swap in that function. A login switch between
  preflight and persistence can therefore matter; root verified this exact source boundary.
- `login/src/auth/storage.rs::FileAuthStorage::save` truncates/writes/flushes the file.
  This path does not atomically replace it. Keyring identity derives from the home; Auto storage
  can fall back to file, while Ephemeral storage is process-local.
- `UnauthorizedRecovery` supports bounded same-account reload then refresh then exhaustion;
  callers drive retries. Refresh source classifies expired/reused/revoked tokens and invalid grant
  as permanent and distinguishes transient failures. These are untested current-service outcomes.
- `login/src/server.rs` implements browser PKCE/callback state/loopback flow;
  `device_code_auth.rs` implements device polling/code exchange. Account restrictions exist.
  `token_data.rs` represents account and plan claims, but cached claims are not current entitlement.
- Ordinary logout deletes local stores and updates cache; `logout_with_revoke` also attempts
  bounded remote revocation and still deletes locally if that attempt fails. Local deletion does
  not prove all issued tokens or already-running requests stopped.

OpenCode at `5a8335857b0ebec44ef6aa1d52b339cf25c329ca` uses a loader-local in-flight refresh
promise and provider credential persistence. Its auth service requests file mode 0600 and removes
local provider entries on deletion. This is a simpler practical precedent, not proof of
cross-process serialization, Windows credential protection or personal-Pro-only enforcement.

## Architecture Options And Edge Cases

A proposed direct implementation can own login and refresh with explicit secure persistence,
account identity and credential generation. The alternative auth-only helper still needs a narrow
credential handoff and single refresh owner; it does not eliminate that ownership question.
Do not create multiple independent refresh owners by copying the same refresh token into isolated
homes. Separate logins are also not assumed to create independent token families without evidence.

Questions for design:

- Define one refresh owner for each credential family across Beryl homes/processes and any helper.
  A process-local lock alone is insufficient if credentials are shared across processes.
- Fence late refresh results against account switch/logout and failed credential persistence.
  If rotation succeeds remotely but saving fails, do not assume the old refresh token remains usable.
- Bind every request to an exact selected account; distinguish account change from token renewal.
  Do not silently move an existing thread to another account or organization.
- Use live account-bound entitlement evidence where available; distinguish confirmed unsupported
  plan from unknown/offline/stale state. Determine mixed-account selection before promising
  personal-Pro-only admission for arbitrary accounts.
- Keep fresh-login identity validation separate from continued use of a service-accepted access
  token. Do not reject working access solely because a cached ID token has expired.
- Define local logout and attempted remote revocation separately, including in-flight requests and
  queued work. Never assume revocation instantaneously cancels remote generation or tool effects.
- Decide where secure credentials/configuration live: current Beryl root authority forbids storing
  backend-owned credentials, so direct ownership requires an explicit authority change.

These are proposed decisions, not new runtime contracts. Direct ownership appears feasible;
no observed authentication obstacle requires retaining CAS.

## Evidence Closure And Deferred Validation

The investigation establishes the mechanism, the observed Pro account path and the main ownership
hazards. It does not justify disruptive live tests on the current account. Rotation/reuse timing,
revocation propagation, downgraded/managed accounts, concurrent login and failure during persistence
remain explicitly unverified. Later design can use bounded refusal/relogin outcomes and locally
tested generation/persistence boundaries without claiming those tests prove server behavior.

Completion review combined source inspection with direct service facts and root validation of
critical refresh/persistence paths. No account/settings mutations, new credentials, software
installation or production source changes occurred.

# Sources

Accessed 2026-09-17:

- Canonical Codex remote `https://github.com/openai/codex.git`, tag `rust-v0.154.0`, exact commit
  `6b9826e3aa83b1a5947db50f4332cb9c65f1b340`:
  [auth manager](https://github.com/openai/codex/blob/6b9826e3aa83b1a5947db50f4332cb9c65f1b340/codex-rs/login/src/auth/manager.rs)
  (`persist_tokens` around 1558, recovery around 1820, refresh around 2789, logout around 2887,
  proactive refresh around 2945),
  [storage](https://github.com/openai/codex/blob/6b9826e3aa83b1a5947db50f4332cb9c65f1b340/codex-rs/login/src/auth/storage.rs),
  [revocation](https://github.com/openai/codex/blob/6b9826e3aa83b1a5947db50f4332cb9c65f1b340/codex-rs/login/src/auth/revoke.rs),
  `codex-rs/login/src/server.rs`, `device_code_auth.rs`, and `token_data.rs`.
- Canonical OpenCode remote `https://github.com/anomalyco/opencode.git`, requested `dev`,
  exact commit `5a8335857b0ebec44ef6aa1d52b339cf25c329ca`:
  [plugin](https://github.com/anomalyco/opencode/blob/5a8335857b0ebec44ef6aa1d52b339cf25c329ca/packages/opencode/src/plugin/openai/codex.ts)
  and [auth storage](https://github.com/anomalyco/opencode/blob/5a8335857b0ebec44ef6aa1d52b339cf25c329ca/packages/opencode/src/auth/index.ts).
- Direct observations linked above, with their exact request conditions and limited conclusions.
- Beryl `doc/design.md`, Persistence and Backend Boundary; authentication analysis is a proposal
  for replacing that boundary, not authorization to change it.
