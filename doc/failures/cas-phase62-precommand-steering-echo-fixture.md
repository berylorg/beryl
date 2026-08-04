# Invalidated Approach

The Phase 37 delayed-steering regression triggered a synthetic CAS user-message echo as soon as
Syndic durably entered `Delivering`, without first observing Beryl's outbound `turn/steer` request.

# Why It Failed

`Delivering` proves that Beryl claimed the durable route; it does not prove that replay,
checked-lifecycle arming, command authorization, or transport dispatch has completed. Scheduler
timing can therefore let the fixture inject the echo before the request that could have caused it.
That wire-impossible ordering can block the provider path instead of exercising post-dispatch
correlation loss.

# Course Correction

CAS fixtures that emit request-caused provider evidence must first read and validate the exact
outbound request. The delayed-steering fixture now observes `turn/steer` and its canonical
accepted-input correlation before sending the deliberately mismatched echo. Durable Syndic state
remains the authority for delivery ownership, but it is not a substitute for transport-dispatch
evidence.
