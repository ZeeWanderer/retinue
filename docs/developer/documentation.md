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

With the preview running, `npm --prefix tools/docs run test:layout` checks the
performance explorer at desktop and phone widths. It uses Playwright Chromium
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

The sidebar is one outline for guides and reference. A crate or module expands
in place; its name opens the overview and its disclosure opens the next level.
The active branch ships in the page HTML. Other branches load as static files.

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

Pull requests build and validate the static site. To publish, run the
Documentation workflow on the default branch with GitHub Pages configured
to use GitHub Actions. A local build does not deploy.
