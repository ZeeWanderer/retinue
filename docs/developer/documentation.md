# Building and authoring documentation

The site combines task guides, language-specific reference, and benchmark data.
Add a guide when the reader needs to accomplish something. Document a function's
contract in its source, not in a second copy of its signature.

## Build locally

Use the project build dependencies, Python 3, Node.js 22.23.4 or newer, and a
docs-only Rust toolchain. The application still builds with stable Rust.

```sh
make docs-toolchain
make docs
make docs-serve
```

Open <http://127.0.0.1:4321/retinue/>. `make test-docs` runs the generator tests;
`make docs-check` validates the built artifact. After changing only a guide or
template, `make docs-site` reuses the extracted metadata.
The performance history build verifies commit links. A shallow clone may need
`git fetch --unshallow` to include the measured revisions.

With the preview running, `npm --prefix tools/docs run test:layout` checks
navigator links and the performance explorer at desktop and phone widths. It uses Playwright Chromium
(`cd tools/docs && npx playwright-core install chromium`).

## Guides and topic groups

Write guides under `docs/`. Register the intended navigation in
[the catalog](../../tools/docs/catalog.mjs). One topic can connect guides and
symbols from multiple modules. Use [topic groups](../../tools/docs/topic-groups.json)
to group free functions, macros or types under a task without changing their Rust
modules. Entries link to existing declarations; unknown entries fail the build.
For example, “Arranging content” groups stack constructors under the UI framework.
Featured symbols need a source doc comment. Put runnable usage examples in Rust
doc comments so the reference shows the same example that Cargo tests.

In the navigator, headings label topics; they aren't folders. Each expandable
collection links to its overview. Put guides beside the declarations they
explain, under the owning framework or application. The rail and breadcrumbs
share that hierarchy. Curated symbols have one home there; recommendations
across frameworks remain links in the article.

In guide prose, put real file names and code identifiers in backticks, for example
`runtime/inbox.rs`, `wfcompanion::runtime::Core`, or `retinue_daemon_app:start/2`.
The site links unambiguous names to their API page; a source file without an API page
links to its repository revision. Command paths, protocol types and qualified
members link to their generated reference. Keep examples and literal values as code.

When a guide uses a short name with several possible definitions, import its
meaning for that guide. To document a domain term once, register its section
in the owning guide. A term can also point to an upstream specification:

```markdown
[api:Canvas]: retinue_ui::drawing::PixelCanvas
[term:InventoryChanges]: #inventory-snapshots-and-deltas
```

Use a kind such as `fn@` before a fully qualified Rust name to distinguish a
constructor from its type. An ambiguous symbol, a wrong import or an unresolved qualified project
reference fails the build. The local
`_build/documentation/unlinked-code.json` lists remaining inline literals by
guide for review; it isn't a list of required links.

For Erlang, use `-moduledoc` for a module's purpose, `-doc` for an exported member's
contract and `-spec` for its types. CLI help describes the command, not its handler.

Link to public repository files or published documentation. Explain how to obtain
inputs that are not distributed; do not cite a private capture, a local checkout,
or a conversation as evidence. Commands may create files in a working directory.
A reader must not need its previous contents to follow a guide.
Local previews include nonignored drafts. The publishing workflow uses
`DOCS_TRACKED_ONLY=1` to reject references to files that have not been added to Git.

## Reference sources

| Reference | Source |
| --- | --- |
| Rust | Pinned Rustdoc JSON: identity, documentation, implementations and re-exports. Source spans and a Rust syntax tree provide declarations without function bodies. |
| Erlang | OTP documentation entries and BEAM abstract forms: functions by arity, types, specs, callbacks and behaviours. |
| CLI | The command tree and its help text; generation does not execute commands. |
| Wire protocol | Protobuf reflection over the maintained schemas. |
| Performance | [Archived measurement JSON](../../tools/benchmarks/history/records/) grouped by compatible series. No handwritten history entries. |

Guides and symbols share navigation and search. Directories under the build
output are generated. Source links use the revision being built.

## Publication

Pull requests build and validate the static site without publishing. Pushes to
`master` publish after validation; the Documentation workflow can also be run
manually on the default branch. A local build does not deploy.
