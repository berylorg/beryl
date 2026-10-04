# Reason For Investigation

Determine whether the pinned Windows GPUI clipboard API can prove copy success before cut and
preflight externally supplied text, metadata and image bytes before allocation.

# Outcome

`App::write_to_clipboard` and the platform trait return unit. Windows discards both the optional
result of opening the clipboard and the inner `Result` from writing it. A caller cannot distinguish
ownership rejection, text-write failure or later metadata-write failure from success. Beryl's
unconditional `Written` adaptation therefore supplies no native success proof.

`App::read_from_clipboard` returns an already allocated `ClipboardItem`. Windows text and metadata
reads construct whole strings; image reads copy the whole `GlobalSize` into a vector. There is no
public caller limit or pre-allocation representation inspection in this path. Checking the returned
item's size is too late to enforce a hard read-allocation bound.

String metadata is supported. Windows accepts it only when the stored text hash matches the visible
text, but that hash is not an application provenance or lifetime capability. The writer publishes
only the first entry, so a multi-entry item is not proof of complete multi-representation publication.
These findings require boundary work before safe native composer clipboard mounting; this note
does not select that architecture or authorize a workaround.

# Sources

- Repository: https://github.com/berylorg/zed-fork.git; requested ref is Beryl's root Cargo pin;
  resolved commit `1664626feb9e4904b7d8bf4016cbed42da558536`, checked by `git rev-parse HEAD`,
  2026-10-04. Local path override and root Git pin agree. Windows target, `default-features = false`,
  `windows-manifest` enabled in Beryl.
- `crates/gpui/src/app.rs`: `write_to_clipboard`, `read_from_clipboard`.
- `crates/gpui/src/platform.rs`: platform clipboard methods and `ClipboardItem` constructors.
- `crates/gpui/src/platform/windows/clipboard.rs`: `with_clipboard`, `write_to_clipboard`,
  `write_to_clipboard_inner`, `write_string_to_clipboard`, `read_string_from_clipboard`,
  `read_metadata_from_clipboard`, `read_image_for_type`, `with_clipboard_data`.
- Local Beryl use: `crates/beryl-app/src/main_window/conversation_composer_owner.rs`,
  `production_clipboard_writer`.
