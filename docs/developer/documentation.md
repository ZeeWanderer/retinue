# Documentation

The site combines handwritten guides with native Rustdoc and ExDoc reference.
Commands are generated from the CLI's help/completion tree; protocol reference
comes from the owning `.proto` files. One search index covers all four.

## Build And Preview

Use the normal project toolchain plus Node.js 22.23.4 or newer and npm.

```sh
make docs
make docs-serve
```

Open `http://127.0.0.1:4321/retinue/`. `make docs` installs locked documentation
dependencies, generates APIs and checks topic links. It does not start the daemon
or run CLI command handlers. ExDoc is a docs-profile plugin, not a runtime
dependency. Erlang, Cargo and native build prerequisites are listed in
[workflows](workflows.md).

For prose edits after the first build, use `make docs-site`. For generator changes,
run `make test-docs` and `make docs-check`. Rust code examples in API comments are
tested with `make test-composition`; rendered examples are not a substitute for
doctests. `DOCS_BASE=/` builds for a domain root instead of the project subpath.

Source Markdown remains readable on GitHub. The builder adds site metadata and
rewrites links into `_build/documentation/content`; it never rewrites source
documents. Final HTML is `_build/docs`, not checked in. Never edit generated
Markdown or HTML.

## What Goes Where

| Content | Owner |
| --- | --- |
| Overview, screenshots and quick install | `README.md` |
| Setup and everyday usage | `docs/installation.md`, game and tool guides |
| Contracts and how-to topics | `docs/developer/` |
| API semantics and examples | Rust documentation comments; Erlang `-moduledoc` / `-doc` |
| Command names, options and help | CLI command tree |
| Wire fields and presence | Owning core's `proto/` schema |
| Reproducible research conclusions | `docs/research/` and linked research articles |
| Site integration and generator tests | `tools/docs/` |

Keep descriptions near the code they explain. Guides cover workflows and
cross-module reasoning, not copied signatures. Document ownership, invalidation,
error behavior and important constraints rather than narrating each line of code.

Rustdoc retains traits, generics, macro documentation, reexports, implementation
links and source navigation. ExDoc retains Erlang module/function/type reference.
Their inner layouts remain native; the portal adds shared navigation and search.
This avoids a custom language renderer or a nightly-only Rustdoc JSON dependency.

## Publication

The Pages workflow builds a static artifact; it does not commit HTML or maintain
a `gh-pages` branch. Enable GitHub Pages with **GitHub Actions** as its source,
then manually run the documentation workflow to publish. Pull requests build and
validate without deploying. No local build publishes anything.

Only documentation sources, reviewed images and generated code reference enter
the artifact. Ignored research and personal workspace data are never copied.
Follow the [research evidence rules](../research/index.md#evidence) before adding
captures. Review both prose and reference changes when a public artifact is built.
