# Authority Decomposition Workflow

This reference is normative when `project-doc-authority/SKILL.md` requires it for decomposing or
reviewing authoritative design documents.

## Review Signal

Document size is a signal to inspect review and retrieval quality, never an automatic split
threshold. A large entry point may remain whole when its decisions are normally read together. A
smaller document may need decomposition when unrelated consumers repeatedly must load contracts
they do not use.

Review decomposition when an authority is difficult to navigate, reviewers cannot isolate a
bounded decision set, unrelated work repeatedly co-reads most of the file, or distinct consumers
need stable subsets. Do not split merely to reduce line or byte counts.

## Preserve Authority Before Pruning

Resolve each contract's owning feature, system, workspace project, or additional declared authority
before reorganizing files. Decomposition cannot repair incorrectly placed authority.

Before deleting or condensing material, inventory the normative contracts, their owner, and the
tasks or consumers that require them. Move or restate every contract in its intended authoritative
location before pruning the old body. Preserve exact semantics, including identities, ordering,
bounds, valid inputs and outputs, failure behavior, recovery, trust, and rigor requirements. Exact
semantic preservation does not require retaining duplicate wording, implementation narration,
derivations, examples, history, or rationale that has no continuing authority value.

If a contract has no clear destination, keep it in the entry point until its ownership is resolved.
Do not use a temporary supplement as an undeclared authority layer.

## Co-Reading Groups

Group decisions by the work that must read them together. Prefer a supplement when it gives one or
more related tasks or consumers a bounded, coherent contract without requiring unrelated
supplements. Keep tightly coupled invariants together even when their subject labels differ.

Do not create fragments around headings, implementation modules, or arbitrary target sizes when a
reader would routinely need to open all fragments. Merge supplements that have the same stable
co-reading set unless separate ownership or a project-declared format requires them to remain
distinct.

## Entry Point And Supplements

The normal `design.md` remains the authoritative entry point and retains its required `# Goals`,
`# Decisions`, and `# Engineering Rigor` structure. It must:

- state the scope's shared and cross-supplement rules;
- link every authoritative supplement and state its bounded semantic role;
- route a reader from a task, consumer, workflow, or contract category to the needed supplement;
- retain decisions that several supplements would otherwise duplicate; and
- define how supplements relate to higher and lower authority without restating those authorities.

A normative supplement owns only the bounded contracts assigned to it by the entry point. It must
not silently broaden its role, redefine shared rules, or duplicate another supplement's contract.
Illustrative and historical files remain non-authoritative unless the entry point explicitly
declares a normative role.

Keep routing concise. Do not turn the entry point into a second copy of every supplement, and do not
force readers to infer routing from filenames alone.

## Selective-Read Validation

After decomposition:

1. Compare the pre-change contract inventory with the entry point and supplements; account for
   every preserved, intentionally changed, or correctly removed item.
2. Validate every entry-point link and role declaration, and verify that each normative supplement
   is reachable from the entry point.
3. Check shared rules and supplement boundaries for duplication or contradiction across the
   applicable authority chain.
4. Walk representative real task or consumer routes from the entry point and confirm that each can
   read the complete applicable contract without opening unrelated supplements.
5. Confirm that a reader who opens only the entry point can still identify the scope, shared rules,
   applicable rigor declaration, and where each bounded contract lives.

Use exact filesystem inspection and direct authoritative reads for these structural checks.
Semantic retrieval may help when a real task has an uncertain route, but synthetic known-answer
semantic queries are not an ordinary decomposition or correctness gate.
