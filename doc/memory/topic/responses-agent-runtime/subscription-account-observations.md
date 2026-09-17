# Reason For Investigation

The replacement needs current personal-Pro admission and quota information without relying on
CAS's normalized account state. Operator authorized small direct subscription-only requests and
read-only use of the existing login, with no competing refresh or account mutation.

# Outcome

A direct authenticated GET to `https://chatgpt.com/backend-api/wham/usage` returned HTTP 200
at 2026-09-17 10:31:23 UTC. Its JSON reported `plan_type: pro`, an account ID matching the
selected login, `allowed: true` and `limit_reached: false`.

Separately, read-only local decoding showed a usable unexpired access token but an expired stored
ID token. Both had a `pro` plan claim and matching selected account ID. These local claims were
not signature-verified and are not independently trusted current entitlement proof. The successful
direct inference and usage responses are separate live evidence that the access token worked.

This is a real lifecycle edge case: an expired stored ID token does not imply that the current
access token cannot call the service. A proposed runtime must distinguish login identity,
access-token validity and current service entitlement rather than treating them as one expiry.
Fresh-login identity validation is a separate responsibility; this finding does not justify
accepting arbitrary unverified ID tokens.

## Direct Method And Observed Shape

Used one one-shot PowerShell/.NET HttpClient request, OAuth Bearer and selected ChatGPT account
headers from the existing login, honest `User-Agent: Beryl-Research-Probe/0.1`, and JSON Accept.
Redirects/cookies were disabled, timeout was 30 seconds, complete response-buffer cap 131,072 bytes,
with no retries or refresh. No inference, credit redemption, plan changes or account enumeration
were requested. The request completed in the approximately 0.84-second shell invocation.

The response was 1,689 UTF-8 bytes with `application/json` content type. Top-level ordered keys:

```text
user_id, account_id, email, plan_type, rate_limit, code_review_rate_limit,
additional_rate_limits, model_usage, credits, spend_control, rate_limit_reached_type,
promo, rate_limit_reset_credits
```

`rate_limit` keys: `allowed, limit_reached, primary_window, secondary_window`.
The primary window exposed `used_percent, limit_window_seconds, reset_after_seconds, reset_at`.
No populated secondary-window keys were reported by the shape inspection. One additional rate-limit
entry was counted; its detailed meaning and model mapping were not inspected in this request.

Account identifiers, email, private usage percentages, credits and raw response bodies were not
printed or retained. Only shapes, selected categorical outcomes and equality checks were reported.
The process disposed its HTTP resources and exited without creating artifacts or changing auth.

## Implications And Limits

The usage response provides a live account-bound plan signal, stronger than a stale local plan
claim. It is still one observation on one Pro account. It does not establish every plan value,
personal-versus-managed selection rule, downgrade propagation, inactive subscription behavior,
refresh semantics, or support for all potential request headers.

Optional limit windows must not be fabricated. The current status-line requirement that account
windows be independently available can be supported by preserving absent/unknown states. Exact
window semantics and additional model-specific limits belong in the inference-surface investigation.

A new auth design should consider account-scoped cache invalidation, the distinction between
unknown/offline and confirmed unsupported entitlement, and safe response to account changes during
refresh. These are proposed decisions for later authority, not implementation rules created here.
Do not force token expiry/revocation or race the live client to manufacture evidence.

# Sources

- OpenAI subscription endpoint `https://chatgpt.com/backend-api/wham/usage`, direct live GET
  on 2026-09-17 at 10:31:23 UTC, with method, bounds and sanitized response facts above.
- Existing login's local token metadata, read-only on 2026-09-17; JWT payloads decoded locally
  without signature verification and with no credential values retained.
- [Direct inference baseline](direct-subscription-baseline.md), independently demonstrated access.
- Official Codex canonical remote `https://github.com/openai/codex.git`, tag `rust-v0.154.0`,
  commit `6b9826e3aa83b1a5947db50f4332cb9c65f1b340`,
  [usage endpoint construction](https://github.com/openai/codex/blob/6b9826e3aa83b1a5947db50f4332cb9c65f1b340/codex-rs/backend-client/src/client/rate_limit_resets.rs),
  inspected 2026-09-17 to identify the existing read-only route. Source suggested the request;
  actual response facts above came directly from the service.
