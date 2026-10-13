# Replacement Import Expectation Capture

An exact mutation request must carry the caller's expected source and gate facts. Capturing
whatever is current during preparation does not reject an already stale caller observation.

Independent implementation review on 2026-10-13 found that historical-import preparation captured
sealed content, historical Asset proof and input-gate revision into its durable basis, but the
request could not express the expected values declared by the owning draft-storage contract.
Its later writer fence was correct; a gate that changed and returned idle before preparation
could nevertheless be silently recaptured and admitted.

The correction adds these three exact descriptive facts to the request and compares them with
authenticated current records before constructing the basis. It changes no canonical basis or
ordinary persisted encoding. Tests must reject stale gate revision, substituted content and
missing, unexpected or wrong Asset proof at preparation, while preserving the existing atomic
writer rejection when qualified facts change after preparation.
