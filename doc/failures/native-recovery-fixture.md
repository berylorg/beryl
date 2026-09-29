# Native Recovery Shell Fixture

On 2026-09-29, extending running-owner resident recovery tests to a real shell exposed two fixture
requirements. The generic fallback appearance requests Inter, which is absent on the native test
machine; run `17cf2a6b-8ccd-4295-9e1c-69b07b0dad36` aborted in DirectWrite. The fixture now uses
the existing system-font appearance helper. No software installation is required.

Activating a hidden shell did not supply the native frame callbacks needed by resident preparation;
run `e352b2f3-2f6d-4250-a4c6-126521ff39eb` timed out with a scheduled frame. The fixture explicitly
publishes its native window before the recovery exercise. This is test setup, not evidence of
production recovery publication.

All 15 focused owner and shell/aggregate cases subsequently passed in run
`bf17c243-9e5c-42ce-8383-7d46bd21d1c3`. Successful fixtures close their homes and windows normally.
The aborted fixtures may have left randomly named homes under the OS temporary directory; their
exact paths were not recorded, so ownership cannot be established for safe deletion. Unidentified
directories were preserved. No test processes remained after verification. Future failing native
fixture investigations should retain exact temporary-home paths before running the GUI callback.
