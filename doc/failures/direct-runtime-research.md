# Direct Runtime Research

## Delivery Uncertainty Is Not Proven Execution Loss

On 2026-09-17, review of the proposed direct-runtime branch assessment found that its wording
terminally failed parent handoff immediately on uncertain delivery. Existing branch authority
prohibits replay immediately but requires proven execution-session loss before converging the
parent incomplete and the job terminally failed. Uncertainty alone could otherwise release the
discussion gate while parent execution remained possible.

Corrected the research note to preserve delivery-unknown custody until authoritative completion
or proven loss, and to retain the `SyncAll` success/archive publication barrier. A future direct
runtime must specify its exact authority-loss boundary; a transport timeout is not that proof.
No production code or live execution was affected.
