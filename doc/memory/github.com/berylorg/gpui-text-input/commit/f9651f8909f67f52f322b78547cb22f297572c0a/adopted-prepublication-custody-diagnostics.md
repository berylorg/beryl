# Reason For Investigation

New Thread recovery qualification needed a real predecessor Page release. A nonempty saved
editor restored through ordinary bootstrap reported zero adopted custody, so the investigation
checked what the diagnostic measures and which construction path actually retains those pages.

# Outcome

`adopted_custody_items` counts only `adopted_prepublication_custody`. It does not count all
regular realized surface or residency pages; `current.resident_pages` is a separate diagnostic.
Nonempty visible text therefore does not establish positive adopted prepublication custody.

At this revision, committing a prepared target publication installs the regular coherent surface
and releases adopted prepublication custody. Widget disposal also releases it through the original
cleanup tokens. Qualification must observe the real adoption before replacement or disposal and
then verify release through the original owner, rather than manufacture a token or broaden normal
construction.

Adopted Page release uses the prepublication cleanup ledger, separately from the ordinary
`RangeTextInputRequest` vector returned by disposal. A widget with adopted pages can legitimately
return no ordinary requests. Positive release evidence must count the original Page token's
authenticated ledger acknowledgement after actual widget release; the ordinary request-vector
length is not a proxy for that protocol.

Beryl's ordinary restored-shell `consume_prepared` passes no prepublication input. Failed-resident
`new_restored` passes the actual prepared environment, candidate and current source to
`RangeTextInput::new_with_prepublication`. A real prior-editor failed-home recovery can therefore
seed adopted Page custody for a subsequent New Thread interruption. The fixture uses that product
route and exact source/history/native assertions. Refresh this finding when the widget pin,
diagnostic definition or either construction path changes.

# Sources

- Canonical remote: `https://github.com/berylorg/gpui-text-input.git`; full resolved commit:
  `f9651f8909f67f52f322b78547cb22f297572c0a`, selected by the Beryl root `Cargo.toml`.
  Accessed 2026-10-08 by focused symbol and source inspection for Windows virtual-GPUI fixtures.
- [`src/range_widget/realization/diagnostics.rs`](https://github.com/berylorg/gpui-text-input/blob/f9651f8909f67f52f322b78547cb22f297572c0a/src/range_widget/realization/diagnostics.rs):
  `realization_diagnostics` distinguishes adopted custody from resident pages.
- [`src/range_widget/prepublication/adopted_custody.rs`](https://github.com/berylorg/gpui-text-input/blob/f9651f8909f67f52f322b78547cb22f297572c0a/src/range_widget/prepublication/adopted_custody.rs):
  `release` readies the original cleanup tokens.
- [`src/range_widget/geometry/response_commit.rs`](https://github.com/berylorg/gpui-text-input/blob/f9651f8909f67f52f322b78547cb22f297572c0a/src/range_widget/geometry/response_commit.rs):
  `commit_prepared_target_publication` releases adoption after installing the regular surface.
- [`src/range_widget/lifecycle.rs`](https://github.com/berylorg/gpui-text-input/blob/f9651f8909f67f52f322b78547cb22f297572c0a/src/range_widget/lifecycle.rs):
  `dispose` releases remaining adopted custody.
- Beryl use sites: `crates/beryl-app/src/main_window/conversation_composer_mount/construction.rs`
  (`consume_prepared`) and the mounted `new_restored` recovery constructor.
