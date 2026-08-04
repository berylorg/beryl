# Reason For Investigation

Phase 26 replaces post-allocation validation of the registered branch-resolution dynamic tool with
incremental enforcement. Its schema declares `maxLength: 65536`, so Beryl needed the authoritative
unit of that limit before choosing decoded-text accounting and process-resource admission.

# Outcome

JSON Schema Draft 2020-12 models a JSON string as Unicode code points and defines `minLength` and
`maxLength` from the number of JSON-string characters, not encoded wire bytes or UTF-8 bytes.

The branch-resolution sink therefore counts decoded Unicode scalar values while parsing. It accepts
at most 65,536 scalars and separately bounds the retained UTF-8 representation at 262,144 bytes,
the maximum possible for that accepted scalar count. JSON escaping and transport fragmentation do
not change the semantic character count.

# Sources

- JSON Schema, "A Media Type for Describing JSON Documents," Draft 2020-12, instance data model,
  published 2022-06-16: https://json-schema.org/draft/2020-12/json-schema-core
- JSON Schema, "A Vocabulary for Structural Validation of JSON," Draft 2020-12, sections 6.3.1 and
  6.3.2, published 2022-06-16: https://json-schema.org/draft/2020-12/json-schema-validation
