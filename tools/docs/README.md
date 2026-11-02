# Documentation Tooling

`make docs` builds the site; `make docs-serve` previews it. See
[authoring and publication](../../docs/developer/documentation.md).

- `prepare.mjs`: topic routing and command/schema reference.
- `cli.escript`: read-only export of the actual CLI tree.
- `protocol.mjs`: Protobuf reflection, not generated-code parsing.
- `native.mjs`: shared navigation/search metadata; native API markup stays intact.
- `check.mjs`: topic links, anchors and API entrypoints.

No generated output is committed. Dependencies are pinned in `package-lock.json`;
Erlang's ExDoc plugin is pinned in the docs profile.
