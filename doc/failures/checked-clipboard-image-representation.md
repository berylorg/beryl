# Checked Clipboard Image Representation

Scope: GPUI checked Windows clipboard implementation, Beryl plan Phase 728.

The attempted image writer required `GlobalSize` to equal the encoded payload length,
returning `Allocation` on any padding. Independent source review invalidated this assumption:
[GlobalAlloc](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-globalalloc)
allocates at least the requested size, and
[GlobalSize](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-globalsize)
can report a larger allocation. Both contracts were checked on 2026-10-04. Consequently a
supported image can be refused even when caller limits admit its actual native backing.
Treating padding as encoded image bytes would also violate exact payload preservation.

Operator approved the correction on 2026-10-04. The accepted representation is a bounded private
companion record containing a version,
image format, exact encoded length and content consistency digest. Charge the record and
actual native allocation to caller limits; publish both representations before acknowledging
success. Checked reads validate the record, length and digest and return exactly the encoded
prefix. Malformed or mismatched companion records refuse acquisition. Foreign images without
the companion remain bounded native representations; their exact logical length cannot be
inferred from allocator size alone. The controlling contract is the
[fork design](../../../zed-fork/doc/design.md#checked-windows-clipboard-boundary).

Qualify nonaligned payloads and padded allocations, missing/malformed companion data,
individual and total bounds, and partial publication failure. The existing deterministic
checks do not substitute for native qualification. The correction passed independent review and
all 23 deterministic cases; [native qualification](../audits/composer-marker-feedback/checked-native-clipboard.md)
remains blocked on safely preservable clipboard preparation after the earlier access denial
cleared. No Beryl GUI was launched.
