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

Compiling these files separately into CLI and daemon applications creates duplicate BEAM ownership,
breaks xref, and makes debugger code paths ambiguous. Keep only stable cross-application contracts
and value helpers here.

## `wfcompanion`

`apps/wfcompanion/` is a standalone Rust process:

- `game_observer`: Warframe process/build identity, read-only memory, and typed evidence.
- `observer`: collector lifecycle and observation policy.
- `daemon`: owner-only Unix socket transport and reconnect replay.
- `relic`: trigger, capture, OCR, item resolution, and market requests.
- `overlay`, `ui`: Warframe scenes, surface orchestration and hit regions.
- `preview`: registered still and animated overlay previews.
- `main`: process composition and Steam wrapper mode.

The companion owns native window and overlay state. It publishes normalized observations to the
daemon and requests shared data from it.

## Native Libraries and Inspector

`apps/retinue-inspect/` builds independently of the companion renderer. `wfinspect` is an alias.

- `crates/retinue-client`: native paths, lifecycle, bounded JSON transport and socket contracts.
- `crates/retinue-ui`: Blend2D painting, Taffy layout, caches, damage and input.
  Separate `overlay` and `desktop` features provide layer-shell and ordinary window hosts.
  Screens, styling and game policy remain in clients.
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

`apps/arccompanion/` provides ARC Raiders launch/discovery scaffolding only. It has no observer,
daemon worker or data store until an actual source is implemented.

## `wfgui`

`apps/wfgui/src/` contains the Rust desktop client, using `retinue-ui` windowing
and `retinue-client` transport; no overlay host or game-memory dependency.
All seven page routes have native implementations; parity work is ongoing.
`resources/` holds GUI assets shared with the
retained Qt implementation under `qt/`. See [native UI](native-ui.md).
Qt reference modules:

- `daemon_client`: local JSON-lines transport, daemon startup, reconnect, and request correlation.
- `relic_model`: typed relic list, filtering, and asset-path updates.
- `relic_grid_widget`: virtualized relic-card delegate and interaction routing.
- `player_item_model`: shared Inventory and Mastery data/filter model.
- `player_item_grid_widget`: responsive model/view card rendering and visible-asset requests.
- `app_controller`: per-view cached metadata, price, asset, loading, and error state.
- `relic_card_layout`: responsive card constraints shared by rendering and tests.
- `main_window` and view widgets: navigation and desktop rendering.

`make gui`, `make dev` and `make prod` build Rust. Qt's CMake/vcpkg build is opt-in
through `make gui-qt-dev` or `make gui-qt-prod`; it outputs `wfgui-qt`.

## Runtime Layout

- Rebar3 and Cargo output: `_build/`
- Download and compiler caches: `.cache/`
- Development stage: `dev/`
- Production stage: `prod/`
- Main escript: `libexec/retinue/retinue.escript` inside each staged prefix
- Daemon release: `libexec/retinue_daemon/bin/retinue_daemon` inside each staged prefix
- Package archives: `releases/`
- Managed user data: inspect with `retinue paths`; game state uses `retinue/games/<game>`
- Tests and fixtures: `apps/retinue_cli/test/`, `apps/wfcompanion/tests/`, `apps/poe2companion/`
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
