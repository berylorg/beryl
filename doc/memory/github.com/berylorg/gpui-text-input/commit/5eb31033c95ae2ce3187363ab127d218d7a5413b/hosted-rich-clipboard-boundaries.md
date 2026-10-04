# Reason For Investigation

Identify the existing bounded provenance and mutation APIs usable by a hosted composer paste,
without treating direct one-marker insertion as general rich selection replacement.

# Outcome

The range clipboard coordinator supports streamed provenance with bounded pages, exact cumulative
identity and compact final closure. The final contiguous write owns no marker-sized provenance
collection. Consumers must stage or discard each page; the text-byte cap alone does not bound the
number of source-zero-width objects. Existing Beryl integration explicitly admits only `Omit`.

Range-backed rich paste configured as `Propagate` emits a command before reading the clipboard.
It does not create a paste operation, capture positions or own a source. The plain-text branch
reads the complete platform value and invokes private `insert_text`.

Public `begin_host_mutation` accepts an exact leased operation, proposal and endpoint proofs.
Evidence page/finish, replay restart, pass page/finish and settlement APIs exist. They provide a
general protocol, but a host must still own captured source, range, proof acquisition and lifecycle.
The public `insert_inline_object_at_selection` accepts a caret or one selected object; a nonempty
text selection returns `Pending`. It cannot implement arbitrary rich paste by itself. Repeating it
would create separate operations rather than the required atomic paste.

# Sources

- Repository: https://github.com/berylorg/gpui-text-input.git; requested ref is Beryl's root Cargo
  pin; resolved commit `5eb31033c95ae2ce3187363ab127d218d7a5413b`, checked by
  `git rev-parse HEAD`, 2026-10-04. Root Git pin and local override agree.
- `doc/design.md`: range-backed editing, clipboard provenance, prepare/commit and terminal release.
- `doc/gui/widgets/text-input/spec.md`: clipboard acknowledgement and hosted editing contracts.
- `src/range_widget/keyboard.rs`: `paste`.
- `src/range_widget/object_edit.rs`: `insert_inline_object_at_selection`.
- `src/range_widget/interaction.rs`: `begin_host_mutation`, `lease_host_operation` and settlement.
- `src/range_widget/mutation_evidence.rs`: public evidence/replay methods.
- Local Beryl use: `crates/beryl-app/src/main_window/conversation_composer_owner/clipboard/collection.rs`
  and `doc/failures/composer-private-clipboard-provenance.md`.
