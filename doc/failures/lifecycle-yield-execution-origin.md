# Scope

Production lifecycle-yield acceptance in the Beryl app, implemented under Phase 352.

# Invalidated Approach

The initial feature handler validated its own healthy stop coordinator and the request's Syndic
thread and turn IDs, but the ordinary dynamic-tool context carried no execution-owner proof.
A handler from another home could therefore accept a request when that home contained the same
thread and turn IDs in an eligible input gate. Registry validation and exact backend routing did
not prove that the injected feature handler belonged to the execution that routed the request.

# Evidence And Correction

Independent review identified the mismatch in `ProcessLifecycleYieldHandler` and
`OrdinaryDynamicToolContext`. The original foreign-home test used different thread IDs and an idle
foreign home, so its rejection did not exercise this defect.

The capture owner now stamps the compact context with its Beryl-home identity, healthy-home
generation and existing process-unique projection-service generation. The feature handler compares
all three before accepting an outcome or minting an attention attempt. The context owns no home,
connection or view resources. The strengthened real WebSocket regression uses matching thread and
turn IDs with eligible work in both distinct homes and requires rejection without foreign state
change.

The owning contracts remain the app feature-adapter and CAS-live exact-authority requirements;
no design change is needed. The matching-ID regression, 72 selected regression/stop checks,
production compilation and final independent review passed. Exact continuation-failure settlement
remains the separately planned Phase 353 boundary.
