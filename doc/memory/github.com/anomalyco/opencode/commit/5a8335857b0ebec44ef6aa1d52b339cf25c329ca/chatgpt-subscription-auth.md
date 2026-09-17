# Reason For Investigation

Operator pointed to OpenCode as a working precedent for using personal ChatGPT subscriptions
without Codex App Server (CAS), including independent OAuth refresh.

# Outcome

OpenCode implements ChatGPT OAuth and direct subscription-backed Responses transport without a
CAS process. This establishes a concrete implementation precedent for Beryl; keeping CAS alive
solely to refresh tokens is not technically required by the inspected implementation.

In `packages/opencode/src/plugin/openai/codex.ts`:

- Browser authorization uses PKCE; a headless device-code login is also implemented.
- `refreshAccessToken` posts a refresh-token grant to the issuer's `/oauth/token`.
- The request wrapper refreshes absent/expired access tokens, shares an in-flight refresh promise,
  and persists the updated access token, refresh token and expiration through its auth service.
- Requests receive OAuth Bearer authorization and the selected ChatGPT account header.
  Responses requests are redirected to `https://chatgpt.com/backend-api/codex/responses`.
- The auth/inference path does not launch CAS or a Codex CLI process.

OpenCode's provider documentation explicitly offers ChatGPT Plus/Pro login. Its implementation
does not establish Beryl's proposed personal-Pro-only admission policy: account selection,
personal context and active plan eligibility still need explicit handling. The plugin's separate
restriction on a model's `pro` reasoning mode must not be confused with subscription eligibility.
Expired-token refresh is demonstrated in source; this inspection does not claim a general
refresh-and-retry response to every HTTP 401.

This is source evidence, not an OpenCode login performed against Operator's account and not a
published support commitment for every third-party client. Neither caveat makes the practical
authentication mechanism hypothetical. Beryl can evaluate this direct mechanism, with an
auth-only official-client helper as a fallback if needed. No architecture was changed.

# Sources

Accessed 2026-09-17:

- Canonical remote `https://github.com/anomalyco/opencode.git`, requested branch `dev`, resolved
  commit `5a8335857b0ebec44ef6aa1d52b339cf25c329ca`; resolution with `git ls-remote`, then
  immutable-source inspection of
  [Codex auth plugin](https://github.com/anomalyco/opencode/blob/5a8335857b0ebec44ef6aa1d52b339cf25c329ca/packages/opencode/src/plugin/openai/codex.ts).
  Relevant regions: authorization around lines 88–101, refresh around 135–149,
  request-time refresh/persistence around 369–396, endpoint/header selection around 413–435,
  and browser/device login methods later in the file.
- OpenCode, [OpenAI provider documentation](https://opencode.ai/docs/providers/#openai),
  for the user-facing ChatGPT Plus/Pro login option.
