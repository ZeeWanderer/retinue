# Entity Architecture

Entities are normalized searchable values built and queried inside `retinue-daemon`, then rendered by
`wfcli`.

## Pipeline

1. `wf_source_manager` validates managed inputs; `retinue_data_cache` performs locked atomic
   fetches.
2. Domain adapters decode source data and derive typed projections; source records remain
   attached and addressable rather than being replaced by parsed output.
3. `wf_entity` and `wf_entity_*` attach `row_map`, sparse `extra_fields`, searchable
   `fields`, and `haystack`.
4. Entity modules own fixed query-field aliases, typed field sources, special matching, and
   default sort semantics. They never create atoms from user input.
5. `wf_entity_query` compiles and evaluates the parsed AST, then sorts and pages results.
6. Daemon services return data maps. CLI formatters use `wfcore` column schemas and CLI-owned
   block presentation specs.

## Ownership

- Daemon entity modules may normalize/search data but must not print or define terminal layouts.
- `wfcore` schemas define column keys, labels, and generic roles needed on both sides.
- `wf_*_presentation` defines block titles and field order.
- Resolvers translate internal identifiers while building daemon entities; raw mode preserves
  source identifiers and UTC values.
- Add a domain entity module when query-field behavior differs. Do not split one only to shorten
  a file.
