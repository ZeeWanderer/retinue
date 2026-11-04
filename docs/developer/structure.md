# Project Structure

Retinue separates platform lifecycle and transport from game-owned services. `wf_*` modules
implement Warframe behavior; platform modules use `retinue_*` names. The `_cli` suffix marks
command handlers; `wfcli` is an executable alias, not an internal module namespace.

## `retinue_cli`

`apps/retinue_cli/` is a process-free OTP application and the `retinue` escript entrypoint.
It composes platform commands with `warframe`, `poe2`, and `arc` subtrees. Executable aliases
select a game before using the same parser, help, and completion tree. MCP shares one stdio
server with explicit platform or game adapters.

- `command/`: arguments, help, completion, exit status, and request construction.
- `client/`: daemon transport, lifecycle, compatibility, autostart, and companion control.
- `mcp/`: stdio JSON-RPC and platform tool dispatch.
- `output/`: JSON, table layout, trees, and terminal handling.
- `games/<game>/`: game commands, domain clients, presentation and MCP adapters.

CLI modules do not fetch, persist, compile query ASTs, normalize worldstate, or run the Forma
solver.

## `retinue_daemon`

`apps/retinue_daemon/src/` owns persistent state and domain work. The OTP application and release
are `retinue_daemon`; the public executable is `retinue-daemon`.

- `runtime/`: shared lifecycle, activity leases, hot update, local transport, and cache helpers.
- `games/warframe/worldstate/`: snapshot fetch, indexing, projection, watches, and identifiers.
- `games/warframe/catalog/`: PublicExport/WFCD updates, exports, knowledge, and source preparation.
- `games/warframe/query/`: query parsing, entity adapters, sorting, and execution.
- `games/warframe/market/`: Market cache/API, relic context, recommendations, and assets.
- `games/warframe/player/`: canonical Warframe player dataset and queries.
- `games/warframe/forma/`: planner queue, search, rules, assignment, config, and model.
- `games/warframe/build/` and `games/warframe/stats/`: saved build groups, plans, and stat calculations.
- `games/poe2/`: native session projections, trade/account domain data, and its game supervisor.

`retinue_sup` owns the shared runtime and separate `wf_sup` and `retinue_poe2_sup`
subtrees. Native clients use one socket with a mandatory game-scoped handshake.
`retinue_connection` owns framing, worker limits and connection lifetime; each game's handler
owns request validation, subscriptions and event projection. Erlang RPC uses explicit game scope.

Daemon replies are data maps. Daemon modules never print, halt, or choose terminal layouts.
Planner and query execution remain daemon-owned so concurrent clients cannot duplicate expensive
work.

## Shared Libraries

`apps/retinue_core/` owns process-free paths, build identity, handshake contracts, text, time,
incident reading and data extraction. `retinue_paths:root/1` selects an XDG platform root;
`retinue_paths:game/2` selects a game namespace.

`apps/wfcore/src/` is the Warframe shared library. It depends on `retinue_core` and has no
application callback, supervisor, or process:

- `contract/`: Warframe datasets, request ownership, paths, and schemas.
- `value/`: polarity and worldstate diff helpers.

`apps/poe2core/` is the corresponding process-free PoE2 contract library. Game-owned
Protobuf schemas live under each core's `proto/`; the common envelope belongs to
`retinue_core/proto/`. Generated Rust models live in `crates/retinue-protocol/`.
Services, fetching and storage remain daemon-owned, not in either game core.

Compiling these files separately into CLI and daemon applications creates duplicate BEAM ownership,
breaks xref, and makes debugger code paths ambiguous. Keep only stable cross-application contracts
and value helpers here.

## `wfcompanion`

`apps/wfcompanion/` is a standalone Rust process:

- `game_observer`: Warframe process/build identity, read-only memory, and typed evidence.
- `observer`: collector lifecycle and observation policy.
- `daemon`: owner-only Unix socket transport and reconnect replay.
- `relic`: trigger, capture, OCR, item resolution, and market requests.
- `presentation`: named overlay views, scene models, drawing and surface/input hosts.
- `domain/media`: asynchronous image preparation and resource leases.
- `preview`: registered still and animated overlay previews.
- `main`: process composition and Steam wrapper mode.

The companion owns native window and overlay state. It publishes normalized observations to the
daemon and requests shared data from it.
See the [overlay source map](companion-presentation.md) for view entry points.

## Native Libraries and Inspector

`apps/retinue-inspect/` builds independently of the companion renderer. `wfinspect` is an alias.

- `crates/retinue-client`: native paths, lifecycle, bounded JSON/Protobuf transport and socket contracts.
- `crates/retinue-protocol`: generated envelope and game-owned native view models. Edit the
  `.proto` files under the owning Erlang app, not generated Rust or Erlang code.
- `crates/retinue-render`: paint programs, optimization, raster resources, damage
  repair, acquired-buffer rendering and Blend2D execution. Owns the native bridge and vendor submodules;
  depends on neither UI authoring nor game policy.
- `crates/retinue-ui`: UI authoring, observable state, environment, layout and input.
  `layout` defines custom layout contracts; `scene` adapts layout to renderer frames.
  `drawing` supplies typed custom drawing and pixel-layout helpers; `runtime`
  retains both authored views and custom drawings. Shared UI examples live here.
  Separate `overlay` and `desktop` features provide layer-shell and ordinary window hosts.
  Hosts use the shared surface renderer for target history, repair planning and
  failure handling, and `presentation_buffers` for Wayland storage and submission.
  Views stay host-agnostic; custom game lifecycle hosts use those mechanisms
  without adopting the SDK event loop.
  Screens, styling and game policy remain in clients.
- `crates/retinue-ui-macros`: compile-time UI authoring. Generated models use the
  UI state runtime; the macro implementation has no runtime dependency on it.
- `crates/retinue-native`: game-independent memory queries, capture/replay, work budgets and reports.
- `crates/retinue-warframe`: game discovery, native adapters, observations, cache and script decoders.

Inspector and Warframe companion consume the same adapters without depending on each other.
Generic memory commands require an explicit PID or capture and need neither a daemon nor game
discovery. Game commands invoke collectors without companion polling policy. Generic Ghidra
exporters live under `tools/ghidra/`; build-specific evidence remains ignored research data.

Its read-only research-memory layer opens either `/proc/PID/mem` or bounded capture blocks behind
the same query functions. Live and captured UI text, references, pointer paths, and typed probes
therefore use one implementation. Snapshot commands emit JSON, watches emit NDJSON, and raw byte
commands keep standard output free of metadata.

Warframe Luau source reconstruction is a separate pinned toolchain stage under
`tools/wf-luau-decompiler/`. The inspector owns cache extraction and build-specific normalization;
the helper owns only normalized-bytecode reconstruction and exposes file/standard-input plus JSON
diagnostics. Staged builds place both executables together so helper discovery needs no config.

Oodle decoding uses the separate `libexec/unoodle` helper. `scripts/build-oozextract`
pins its registry archive and checksum, builds with the caller's Rust flags, and
prepares its source and license. `scripts/build-companion` stages them under
`share/licenses/oozextract/` with the completed companion build.

## `poe2companion`

`apps/poe2companion/` owns Path of Exile 2 observation and publication to the daemon's
`session` interface. `crates/retinue-poe2` owns process discovery, PE bindings and
bounded typed readers, shared with `retinue-inspect poe2`. Generic memory/capture
mechanics remain in `retinue-native`; companion lifecycle/reload mechanics live in
`retinue-client`. See the [PoE2 guide](../poe2.md).

Its `presentation` folder owns the overlay host and Evaluate views/model;
`domain` owns request workers, image preparation and preferences. Plain evaluation
records live in `data`. See the [overlay source map](companion-presentation.md).

`apps/arccompanion/` provides ARC Raiders launch/discovery scaffolding only. It has no observer,
daemon worker or data store until an actual source is implemented.

## `wfgui`

`apps/wfgui/src/` contains the Rust desktop client, using `retinue-ui` windowing
and `retinue-client` transport; no overlay host or game-memory dependency.
All seven page routes use the retained SDK. There is one layout/paint path;
custom drawings are SDK primitives rather than a fallback scene.

- `main.rs`, `app.rs`: entrypoint, window lifecycle, task delivery and captures.
- `data/`: plain data models and serialized preferences. Canonical game models
  come from `retinue-protocol`; clients do not define competing schemas.
- `domain/`: shared client services, sessions, image leases, preference storage
  and asynchronous workers. Game fetching and calculation stay in the daemon.
- `presentation/`: feature-grouped views, layouts, controllers and adjacent
  observable models. Start with `shell/shell_view.rs`, then a page's `*_view.rs`.
- `presentation/widgets/`: reusable visual components, search and menu tools.
- `presentation/controls/`: application input/editor state and SDK paint-host
  integration, not another widget/layout framework.
- `resources/`: application assets; shared assets remain in their shared owner.

Use one file per meaningful View or Layout, named for that type. Keep a page's
model beside its view. Add subdirectories for cohesive features, not an empty
layer for every kind of file. Shared services are application-owned and injected;
the `domain` directory is not a requirement for global singletons. See the
[GUI source map](gui-organization.md) for concrete entrypoints.

`make gui`, `make dev` and `make prod` build the Rust client. Historical Qt source
remains in Git history, not as a parallel implementation in the working tree.

## App-Local Foundations

An optional `core/` holds cohesive app-local, SDK-adjacent value helpers or
extensions that have no game or presentation-feature knowledge. It must not
depend on the app's `data`, `domain` or `presentation` layers. Reusable mechanisms
belong in the existing shared crates instead. Keep small feature-local helpers
beside their consumer; do not create an empty `core` or a generic utilities bin.

Current native apps need no such directory: drawing/input foundations already
belong to the SDK, while media, preferences and session workers are domain services.

## Runtime Layout

- Rebar3 and Cargo output: `_build/`
- Download and compiler caches: `.cache/`
- Development stage: `dev/`
- Production stage: `prod/`
- Main escript: `libexec/retinue/retinue.escript` inside each staged prefix
- Daemon release: `libexec/retinue_daemon/bin/retinue_daemon` inside each staged prefix
- Package archives: `releases/`
- Managed user data: inspect with `retinue paths`; game state uses `retinue/games/<game>`
- Tests: owning app/crate; shared fixtures/support and cross-app scenarios in `test/`.
  See [testing](testing.md).
- Disposable upstream research: ignored `research/`

## Change Placement

New worldstate type: project/index in `apps/retinue_daemon/src/games/warframe/`, extend `wfcore`
only when clients need the contract, add CLI presentation, then add fixture/output tests.

New daemon feature: use an existing queue unless independent state or failure isolation requires a
new supervised worker. Put domain services in their game subtree. Return data-only replies and
implement `code_change/3` for stateful changes.

New CLI command: parse options, submit a typed request, render the reply. Query text stays opaque
until the daemon parses it.

New MCP operation: translate a schema into an existing typed daemon request. Add a daemon request
only when no suitable operation exists.
