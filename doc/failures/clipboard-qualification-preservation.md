# Clipboard Qualification Preservation

Scope: isolated GPUI native clipboard test harness.

The original shared harness checked sequence only before final restoration. A foreign replacement
before any earlier write could be overwritten, so that final fence did not protect the Operator's
clipboard. Adding a sequence check before every mutation corrected that gap but still assumed the
writer's held sequence would remain valid after closing the clipboard.

Native run `7abb29ed-4a41-4bad-b0aa-8749e34aa006` observed read sequence 2518 versus publication
sequence 2515 and refused stale restoration. A subsequent content-free trace observed close-time
sequence 2522 becoming 2525 and format count three becoming six. Windows format synthesis is
consistent with this evidence, but owner equality cannot exclude foreign additions that retain the
previous owner. Adopting the changed sequence would weaken preservation. The Operator actively uses
the clipboard, so its contents cannot serve as a stable qualification fixture.

The accepted test correction creates its own unique window station and desktop before any clipboard
operation. It never acquires or modifies the Operator clipboard. Cleanup destroys its message-only
owner window, restores and verifies exact borrowed process/thread bindings, and closes only owned
handles. The obsolete shared restoration helper and its three cases were removed. Independent
review, focused compilation and all 20 current deterministic cases pass. Operator-authorized inline
sudo qualified the private native case, including binding restoration and owned-handle cleanup.

Retain native sequence semantics in the
[research note](../memory/topic/windows-clipboard-qualification/native-sequence-and-private-station.md).
The [qualification evidence](../audits/composer-marker-feedback/checked-native-clipboard.md) owns
the current resume command and remaining acceptance gates.
