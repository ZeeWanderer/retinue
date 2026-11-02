# Contributor Documentation

User-facing setup and usage start in [`README.md`](../README.md) and the top-level guides under
`docs/`. This index is for contributors. Documents under `docs/developer/` describe current
ownership and implementation unless their title explicitly identifies them as research or an
audit.

## Architecture

- [`structure.md`](developer/structure.md): applications, native companion, and ownership
  boundaries
- [`daemon.md`](developer/daemon.md): lifecycle, supervision, queues, compatibility, and updates
- [`entities.md`](developer/entities.md): normalized entities and the daemon/CLI rendering boundary
- [`shared_utilities.md`](developer/shared_utilities.md): stable cross-application contracts
- [`cli.md`](developer/cli.md): command dispatch, parsing, and terminal output
- [`companion.md`](developer/companion.md): native observer, local protocol, player data, and overlay
- [`gui-organization.md`](developer/gui-organization.md): Rust GUI source map and ownership
- [`companion-presentation.md`](developer/companion-presentation.md): overlay views, models and workers
- [`ui-sdk.md`](developer/ui-sdk.md): retained views, observable state and custom layout
- [`render-pipeline.md`](developer/render-pipeline.md): paint programs, optimization and repair
- [`player-data.md`](developer/player-data.md): account payload, collector, and indexing boundary
- [`assets.md`](developer/assets.md): embedded and dynamic texture ownership

## Data And Queries

- [`worldstate.md`](developer/worldstate.md): fetching, indexing, translation, and watches
- [`exports.md`](developer/exports.md): official PublicExport files and loading
- [`knowledge.md`](developer/knowledge.md): official Codex and WFCD catalogs
- [`query-language.md`](developer/query-language.md): grammar, AST, schemas, and evaluation
- [`table_layout.md`](developer/table_layout.md): terminal table layout
- [`forma_plan.md`](developer/forma_plan.md): planner queue, model, search, and tests
- [`visualize.md`](developer/visualize.md): Forma visualization

## Development

- [`workflows.md`](developer/workflows.md): build, test, fixture, and metadata routines
- [`adding_features.md`](developer/adding_features.md): checklist for features and data sources
- [`tools.md`](developer/tools.md): inspection and benchmark helpers
- [`documentation.md`](developer/documentation.md): generated reference, topic authoring and site builds

## Research And Audits

- [`wfcd-parser-audit.md`](developer/wfcd-parser-audit.md): differences from reviewed WFCD parsers
- [`alecaframe-overlay-catalog.md`](developer/alecaframe-overlay-catalog.md): upstream overlay
  catalog and local status
- [`overwolf-alecaframe-overlay-research.md`](developer/overwolf-alecaframe-overlay-research.md):
  acquisition mechanisms, Wayland design, safety boundary, and evidence

MCP adapter behavior is documented in [`mcp.md`](mcp.md).
