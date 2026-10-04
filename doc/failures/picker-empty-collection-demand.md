# Picker Empty Collection Demand

The mounted Running threads search fixture exposed an unbounded same-revision request loop:
an accepted empty collection requested its initial page during viewport realization, but the
collection's request admission rejected only positions already represented by resident rows.
An empty collection has no such row, so position zero kept being requested and settled. GPUI
could remain inside draw processing without reaching the fixture's wall-clock deadline.

Request admission must also enforce the admitted collection's total. A current collection rejects
positions at or beyond that total, including zero for an empty result. An unadmitted query may
still request the initial discovery page. This keeps query discovery distinct from viewport demand
over an already accepted revision, without changing search or collection authority.

The corrective boundary is the canonical picker's collection model and mounted empty-search
regression, under the conversation-threads GUI and bounded scroll-surface contracts. Qualification
remains part of the active root plan.
