# Daemon Architecture

[api:retinue_daemon]: app:retinue_daemon

The [daemon application](https://zeewanderer.github.io/retinue/reference/erlang/retinue_daemon/api-reference/)
hosts shared runtime services and separate game supervisors. `retinue_daemon_app:start/2`
starts `retinue_sup`; `retinue_games_sup` owns the per-game supervisors. The public binary
is [`retinue-daemon`](../daemon.md). Warframe services own fetching, worldstate, catalogs, queries, player state,
Market, Forma planning, and watches. PoE2 services own native session snapshots and events.
The [`retinue_cli` module](https://zeewanderer.github.io/retinue/reference/erlang/retinue_cli/retinue_cli/) composes command scopes; the Warframe CLI modules own arguments, MCP framing,
and formatting. Native observation stays in each game's companion.

## Lifecycle And Transport

- Erlang clients connect to the [configured daemon node, normally `retinue_daemon@localhost`](https://zeewanderer.github.io/retinue/reference/erlang/retinue_cli/retinue_client/#daemon_node/0).
- Distribution binds to IPv4 loopback and uses the invoking user's Erlang cookie.
- OTP 29 dynamic hidden client nodes do not listen for incoming distribution connections.
- Client/daemon requests use [`gen_server:call({retinue_daemon, Node}, Request, Timeout)`](https://www.erlang.org/doc/apps/stdlib/gen_server.html#call/3).
- Synchronous calls send [`{game, Game, Request}`](https://zeewanderer.github.io/retinue/reference/erlang/retinue_cli/retinue_client/#call/2).
  Asynchronous requests carry the Erlang atom key [`game`](https://zeewanderer.github.io/retinue/reference/erlang/retinue_cli/retinue_client/#one_shot/2) in the request map.
  Platform status keeps per-game state under [`games.<game>`](https://zeewanderer.github.io/retinue/reference/erlang/retinue_daemon/retinue_games/#status/0); unscoped domain requests are rejected.
- Production code contains no direct [`rpc`](https://www.erlang.org/doc/apps/kernel/rpc.html),
  [`erpc`](https://www.erlang.org/doc/apps/kernel/erpc.html), or remote
  [`spawn`](https://www.erlang.org/doc/apps/erts/erlang.html#spawn/4) evaluation.
- Concurrent startup tolerates another client winning the race.
- Implicit startup uses idle shutdown; explicit startup reapplies persistent or requested idle
  policy to a running daemon.
- Watches, queued work, and connected companions hold an activity lease.
- Forma plans are serialized because one planner already uses internal CPU parallelism.

The daemon changes its working directory to [`$XDG_STATE_HOME/retinue`](../daemon.md#logs) during application startup.
This keeps crash dumps and relative runtime state valid while a staged release directory is
replaced.

## Build Identity

Each transport has a stable handshake envelope and independently versioned interface groups.
Clients require exact versions for groups they use and negotiate optional features. Artifact
[flavor (`dev` or `prod`)](workflows.md#targets) prevents one environment from starting or updating the other. Build
identity is SHA-256 over sorted module names and BEAM MD5 values for `retinue_core`, `wfcore`,
`poe2core`, and `retinue_daemon`.

Handshake recovery:

1. Compare required interfaces, flavor, and build identity.
2. If the daemon contract is older, hot-load the calling artifact. An older client never
   downgrades a newer daemon contract.
3. Repeat handshake.
4. Stop the old daemon and start the requested release if compatibility is still not established.

Repository applications are versioned and released in lockstep. Contract discipline:

- Change the envelope only when handshake or framing changes.
- Increment only the interface group whose request, response, event, or semantics break.
- Additive fields do not change an interface version.
- Optional behavior uses a negotiated feature; absence disables that behavior.
- Support one exact current interface version. Delete replaced code in the same change; do not add
  ranges, adapters, fallbacks, or old-version test matrices.
- Update daemon, CLI, MCP, GUI, companion, and contract tests together when affected.
- Restart native socket connections after local envelope or interface definitions change.
- Migrate non-recoverable user state such as saved build groups. Invalidate recoverable
  observations, catalogs, asset caches, and derived views when migration adds needless code.

## Requests And Cancellation

One-off commands use `retinue_daemon:submit/2` for a monitored one-shot subscription. The daemon queues work, sends exactly
one `{retinue_daemon, Ref, Reply}` message, and removes the request. Persistent watches use the same
shape but stay registered.

The local client relay is monitored across distribution. Closing a terminal, killing a client, or
MCP cancellation produces a [`DOWN` monitor message](https://www.erlang.org/doc/apps/erts/erlang.html#monitor/2); queued or active query, market, catalog, and planning work is
released without polling. Shared worldstate fetches may continue when other clients or cache
refresh still need them.

Companion diagnostic streams use consumption credit and expire. The local socket owner
correlates requests to one companion and monitors the CLI relay; disconnects cancel the
watch instead of resubscribing it. See [companion transport](companion.md#local-contract).

## Native Game Scopes

[api:session]: retinue_local_protocol:interfaces/1
[api:get]: retinue.poe2.v1.GetSessionRequest
[api:publish]: retinue.poe2.v1.PublishSessionRequest
[term:trade.prediction]: retinue_local_protocol:features/1
[term:trade_predict]: retinue.poe2.v1.TradePredictRequest
[term:trade_item]: retinue.poe2.v1.TradeItemRequest
[api:text]: retinue.poe2.v1.TradePredictRequest.text
[api:league]: retinue.poe2.v1.TradePredictRequest.league
[term:prediction_available]: retinue.poe2.v1.TradeEvaluation.prediction_available
[api:estimate]: retinue.poe2.v1.TradePrediction.estimate

All native clients connect to [`$XDG_RUNTIME_DIR/retinue/daemon.sock`, or the owner-only
`/tmp/retinue-UID/daemon.sock` fallback](../daemon.md#native-socket). `RETINUE_DAEMON_SOCKET` can override the endpoint.
The [initial `hello`](companion.md#local-contract) is JSON, not Protobuf. Its [`game` key](https://zeewanderer.github.io/retinue/reference/erlang/retinue_daemon/retinue_local_protocol/#negotiate/1) must be `warframe` or `poe2`. The connection
keeps that scope for requests, subscriptions, publications, and cleanup. Missing or unsupported
games fail negotiation; a client cannot access another game's interfaces through its session.

Warframe negotiates its existing domain interfaces. PoE2 negotiates `session` version 2 for
`get`, `publish`, and subscriptions. Interface versions and optional features are game-scoped;
the shared framing envelope is not a declaration of equivalent capabilities across games.

PoE2's optional `trade.prediction` feature enables `trade_predict` with copied item
`text` and `league`. `trade_item` reports `prediction_available`; the daemon owns
payload normalization and upstream access. A separate supervised worker coalesces
identical predictions, holds at most 128 cached results for ten minutes, and backs
off on unavailable models, rate limits and failures. Prediction waits do not hold
the trade or session workers. Replies contain `estimate` (or null with a reason),
with optional low/high/confidence uncertainty. No raw clipboard/account data leaves
the daemon; only allowlisted item fields go to the fixed prediction host.

`retinue_paths` owns platform XDG roots and per-game namespaces. Warframe's `wf_paths` remains
an adapter. `RETINUE_BUILD_FLAVOR`, `RETINUE_INSTALL_ROOT`, and `RETINUE_UPDATE_ROOT` identify
the staged runtime; game-specific settings keep their existing names.

[term:RETINUE_BUILD_FLAVOR]: retinue_build:flavor/0
[term:RETINUE_INSTALL_ROOT]: retinue_build:install_root/0
[term:RETINUE_UPDATE_ROOT]: retinue_build:update_root/0

## Worldstate And Catalogs

`wf_worldstate_service` caches decoded snapshots by cache path and indexes by normalization
options. With active watches, one global timer polls worldstate; every due watch evaluates against
the same parsed snapshot. Fetch failure retains the last good snapshot and marks delivery stale.

`wf_exports_store` runs concurrent queries against shared catalog generations. A
generation contains individually encoded rows; workers retain sort keys and offsets
while scanning, then decode only the returned page a second time. Concurrent misses
for the same dataset share one build. File identity, modification time and size
invalidate a generation; existing readers finish against their original binary.

`wf_source_manager` checks managed sources on demand and periodically while a
Warframe companion or GUI is connected. Daemon startup, status checks and PoE2
connections do not schedule Warframe maintenance. Explicit updates still work
without a game. Custom source directories are read-only. After one minute without
catalog queries or player-view usage, the respective cache releases its derived
data; saved groups, player snapshots and downloaded sources are unaffected.

Query text reaches the daemon uncompiled. `wf_query_parse` builds the AST; entity modules expose
typed fields and retain raw data paths where a parser has not added semantics.
Catalog query replies contain [`slice`, `total`, and `shown`](query-language.md#paged-results);
the complete matching set stays inside the daemon.

## Market And Player Data

[term:inventory_http]: wf_player_inventory:update/4
[term:inventory_native]: wf_player_inventory:update/4
[api:inventory]: wf_player_inventory

`wf_market_service` owns Warframe Market manifests, base and variant quote TTLs, persistence,
and coalescing. `wf_market_account_service` serializes authenticated order mutations;
`wf_market_presence_service` owns account status. Public and authenticated HTTP work share
`wf_market_limiter`. API modules handle wire normalization and caches use atomic persistence.

`wf_player_service` owns canonical local observations by source namespace. `retinue_local_api`
accepts owner-only Unix socket clients, including `wfcompanion`, and removes subscriptions on
disconnect. Persisted observations do not keep the daemon alive.

`inventory_http` retains the last full response; `inventory_native` retains absolute
resource, blueprint and foundry collections. `wf_player_inventory` validates and
combines them into `inventory` in one revision. Native reads must match the full
baseline's boot, process/session, stream, sequence and sync, and start after its
capture. Older or replayed observations cannot replace newer state. Reconciliation
preserves unknown row fields; unavailable reads never mean empty collections.
Native clients cannot publish `inventory` directly. Query's default player search
uses the canonical inventory; raw source namespaces remain explicitly queryable.

Player subscriptions support full snapshots, revision-only metadata, or a small
companion HUD view. The store projects before sending, and publish acknowledgements
request metadata only. HUD subscribers receive game/collector changes and clears,
not unrelated inventory updates.

Native clients correlate requests by ID. The initial JSON-line handshake requires
[`transport: "protobuf"` and `native` version 1](https://zeewanderer.github.io/retinue/reference/erlang/retinue_daemon/retinue_local_protocol/#contract/1). After its JSON acknowledgement,
every session uses a four-byte big-endian length followed by
`retinue.wire.v1.Packet`. Requests, replies, acknowledgements, failures and events have distinct
typed envelopes. Game payloads carry a fully qualified schema name and Protobuf bytes; there is
no generic JSON packet fallback. Receivers bound lengths before allocating and retain
partial frames across cancellation. A failed schema check is an error, never an empty dataset.

`wfgui`, `wfcompanion` and `poe2companion` use this transport. The GUI retains Foundry,
Inventory, Mastery, Relics and equipment as generated records. `apps/wfcore/proto/`
and `apps/poe2core/proto/` own their game contracts; `apps/retinue_core/proto/` owns
the envelope. [`gpb`](https://github.com/tomas-abrahamsson/gpb) generates Erlang boundary code;
[`prost-build`](https://docs.rs/prost-build/) and [`protox`](https://docs.rs/protox/) generate Rust
models in [`retinue-protocol`](../../crates/retinue-protocol), without a system
[`protoc`](https://protobuf.dev/installation/) dependency. Neither gRPC nor a second
hand-maintained field mask is involved. The schema is a client view, not the canonical store:
raw game documents, diagnostic extensions and open-ended calculator results use explicit
`Document` fields. They are not substitutes for known view schemas. CLI calls and subscriptions
use distributed Erlang native terms, independently of the native-client socket; user-facing CLI
JSON, query and MCP output remain unchanged. Optional scalar and collection presence matters for
patches and trade filters; test absent, null, zero, false and empty values separately.

`scripts/protobuf-deps` invalidates generated Erlang code when cross-app schema imports or
generator configuration change. Generated sources are ignored; both languages build from the
checked-in schemas. Update all affected clients and daemon contracts together.

The daemon already stores decoded native Erlang maps/lists. Do not replace domain state with
wire messages or reparse JSON to project it. Serialize large replies in bounded request workers,
then pass encoded binaries to the connection owner. Keep only one generation of each derived
player view; a late worker must not overwrite a newer reservation or repopulate an invalidated cache.
Independent reads remain concurrent; mutations and rate-limited fetches stay with their owners.
GUI and companion can reconcile current decode failures through the optional diagnostics feature;
the daemon stores them with its own name, metadata, and asset failures.

Native sessions monitor their backend owners. A backend restart closes affected sessions;
clients reconnect and reconcile snapshots instead of retaining dead subscriptions or requests.

## PoE2 Sessions

[api:social]: retinue.poe2.v1.Snapshot.social
[api:entities]: retinue.poe2.v1.Snapshot.entities
[term:unique_monster_death]: retinue_poe2_session:monster_deaths/5

`retinue_poe2_service` validates native samples through `retinue_poe2_session` and
derives bounded change events. Samples carry stream/sequence identity, observation
time, process identity, status, snapshot and errors. Retries are deduplicated;
unchanged heartbeats refresh observation time without disk writes or notifications.
Changed snapshots are persisted before acknowledgement, then published to subscribers.

Schema 2 stores the current snapshot and up to 128 events in
[`$XDG_CACHE_HOME/retinue/games/poe2/session.json`](https://zeewanderer.github.io/retinue/reference/erlang/retinue_daemon/retinue_poe2_service/#init/1). Death requires a known positive
life value followed by zero for the same character and process. Missing reads,
disconnects and cached snapshots do not establish death or a running game.

The optional `social` snapshot contains party members and a local account/league
identity only when the reader finds an unambiguous matching character. It does
not imply web authentication. Missing social reads remain unknown and report
health failures independently of player/area data. The full social directory is
not published or persisted. This additive field keeps the session interface at 2.

The optional `entities` observation contains a bounded entity count and qualifying
unique monsters (ID, address, name, current life), with tree/area identity.
`unique_monster_death` requires positive life followed by zero across consecutive
valid observations (no more than 65 seconds apart) of the same entity, character,
stream, process and area.
Disappearance is not death; the event makes no claim about the killer.
Unknown reads report an independent entity-adapter issue. Inspector captures and
replays use the same reader. Empty monster lists validate traversal, not the
live behavior of monster-specific fields.

## PoE2 Trade

[term:quick_selected]: retinue.poe2.v1.Modifier.quick_selected
[term:exact_selected]: retinue.poe2.v1.Modifier.exact_selected
[term:base_selected]: retinue.poe2.v1.Modifier.base_selected
[term:trade_search]: retinue.poe2.v1.TradeSearchRequest
[term:trade_restore]: retinue.poe2.v1.TradeRestoreRequest
[term:trade_fetch]: retinue.poe2.v1.TradeFetchRequest
[api:next_offset]: retinue.poe2.v1.TradeResults.next_offset
[api:results]: retinue.poe2.v1.TradeRestoration.results
[term:auto_search_allowed]: retinue.poe2.v1.TradeRestoration.auto_search_allowed

PoE2 trade parsing, category defaults, modifier ranking and query construction live
in the daemon. The companion consumes profile choices, selected rows and numeric
bounds. `retinue_poe2_catalog` lazily caches RePoE base items, mods and stat mappings
at [`$XDG_CACHE_HOME/retinue/games/poe2/trade/enrichment.json`](https://zeewanderer.github.io/retinue/reference/erlang/retinue_daemon/retinue_poe2_catalog/#load/1). A bounded worker serves
cached data before refreshing; failed refreshes preserve it. Cold requests wait at
most five seconds for enrichment. Missing internal-stat mappings omit those bonuses,
not known clipboard tags or rolls. Ambiguous trade modifiers remain unsupported.
Catalog warnings are retained in the daemon incident log.
`retinue_poe2_trade_properties` derives quality-normalized DPS/defence filters and
maps known local-stat identities to their properties. Profiles rank properties
and modifier groups together and provide `quick_selected`, `exact_selected` and
`base_selected`. The client must not reinterpret Exact Match as enabling every row.

Trade interface 5 separates `trade_search`, `trade_restore` and `trade_fetch`. Search returns its
ID, URL, counts and `next_offset`; the daemon retains the upstream listing IDs.
Fetch accepts that search ID and an offset, returning up to ten normalized listings
and the next offset (or `null`). Only daemon-issued, retained searches can be fetched.
Offsets advance by requested IDs, including deleted listings, not by displayed rows.
Returned rows follow the search order. The client rejects replies from superseded
requests and starts the next batch when the third-from-last row becomes visible.

The daemon retains the ten most recently completed queries, each with at most
10,000 IDs and eight cached pages. Keys use the canonical query and league; the
current trade client is English-only. This is an in-memory session cache without
time expiry. Reads do not change eviction order; the first page is retained when
later pages reach the bound. Explicit refresh replaces a query and its pages only
after a successful response.

`trade_restore` accepts the same query inputs and returns `results` (or `null`) and
`auto_search_allowed`, a current rate-budget check. It uses cached metadata without
network refresh, even while another upstream request is pending. Results include
contiguous cached pages and the next missing offset. The companion restores these
before considering automatic search; manual Search/Refresh bypasses the cache.
Search and fetch share the existing upstream rate-limit accounting. Failed fetches
do not discard search metadata or previously displayed rows, and are not retried
automatically. The overlay displays cooldowns using a monotonic deadline.

Long-lived process state must use [external callbacks (`fun Module:function/Arity`)](https://www.erlang.org/doc/system/funs.html)
or MFA tuples, not local functions from a reloadable module. Code purging invalidates
local functions; [`code_change/3`](https://www.erlang.org/doc/apps/stdlib/gen_server.html#c:code_change/3) must refresh module-owned callbacks when necessary.

## Build Groups

[api:build_stats]: retinue_local_protocol:interfaces/0
[api:forma_cost]: retinue.warframe.native.v1.BuildPlan.forma_cost
[api:forma_count]: retinue.warframe.native.v1.BuildPlan.forma_count
[api:forma_requirements]: retinue.warframe.native.v1.BuildPlan.forma_requirements

`wf_build_service` owns saved groups and immutable member snapshots. Mutations are acknowledged
only after an atomic, synchronous state-file save. A rejected state file remains untouched and
group operations report the error. Durable groups live at
[`$XDG_DATA_HOME/retinue/games/warframe/builds.term`](https://zeewanderer.github.io/retinue/reference/erlang/retinue_daemon/wf_build_store/#path/0). Refetchable source revisions and plan results
live separately in XDG cache.

Physical targets are resolved from current player equipment when read or planned. Plans are keyed
by group revision, target fingerprint, and source catalog identity; completion rechecks all three.
Missing targets retain their groups but require retargeting before calculation.

Build-stat requests share the monitored build worker pool but neither depend on Overframe nor
cache mutable player results. `wf_stat_buildset` resolves slot-keyed owned selectors or inline
normalized builds. `wf_stat_profiles` validates reviewed attacks against PublicExport;
`wf_stat_rules` and `wf_stat_upgrades` describe reviewed mechanics and ranged mods.
`wf_stat_effects` compiles source/target-filtered contributions and their dependencies.
`wf_stat_math` evaluates numeric
stages; `wf_build_stats` coordinates inputs and reports coverage. CLI and native clients use
the `build_stats` interface. Clients never interpret raw game slot order or apply combat formulas.
`wf_stat_math:scalar/2` evaluates an ordered upgrade bucket, stopping at its first `SET`.
`wf_stat_math:scalar_updates/2` applies successive accumulator writes; later `SET` writes replace earlier ones.
Its `settle` marker evaluates and resets accumulator state at a native stage boundary;
do not collapse intermediate modifiers to floats before that boundary.
Keep traversal policy separate from arithmetic when composing hit-proxy or packet stages.

A buildset can contain one item or the equipment for every slot. Ownership is optional for inline
builds; a target selects the evaluated item. Effects retain source identity, target scope and
activation conditions. Inactive, explicit and lower/upper states use the same evaluator. Unknown
contextual effects are coverage gaps, not assumed absent. Held-state bounds do not establish
combat uptime or prove that all maxima coexist.
Attack profiles require native evidence; a trigger label alone does not establish hitscan or
reload behavior. PublicExport damage is per projectile; apply multishot once. Respect upgrade
family and class filters, including nested conditional entries. Only primary elements pair;
compound-element mods retain their own channels.
Ability buffs evaluate after their source strength/duration modifiers and declare those
dependencies. Unknown ability ranks are coverage gaps, not implicit max-rank defaults.
Frame properties use their own profile and stat output; ability results carry units and
variants, not a universal damage scalar. Reuse shared operators and declarative rule data.
Add a mechanic adapter only for distinct script/native behavior, not for another item
using the same operations. Active abilities come from PublicExport; a Helminth override
replaces its native slot. Never infer all ability values scale linearly with strength.
New conditional rules must define valid endpoints; conflicting or non-monotonic effects cannot
use simultaneous caps as an automatic damage maximum.

New effects require source evidence, rank/activation rules and deterministic tests. Unknown
inputs remain visible as incomplete coverage; missing base stats are errors. Preserve rule and
game fingerprints, and declare research dependencies in the wfinspect feature manifest. Matching
fingerprints do not establish live damage parity.
Rules expose their mechanic ID in result provenance. Shared operations belong in shared mechanic
dependencies; retain property inheritance and unresolved resource references in update reviews.
The daemon never runs extractors or Ghidra during a calculation. Research tools detect drift;
reviewed code implements the formulas.

The daemon supplies executable operations and complete normalized loadouts, including final
slots, polarities and drain. Clients use the shared topology renderer, without source-specific
repairs. `forma_cost` is the weighted search objective; `forma_count` and `forma_requirements`
describe actual consumables.

Flexible weapon assignments preserve the signature of basic-element pairs in visual row-major
order. Duplicate types merge; combined-element and non-elemental mods do not enter this sequence.
Never treat unresolved effect metadata as an empty list. Exact slot locking bypasses rearrangement,
not the physical target's current polarities. Partial capacity checks must be optimistic; validate
complete plans using the same assignment implementation used to produce their loadouts.

## Supervision

`retinue_sup` owns shared activity tracking, daemon control, local transport, and the
`wf_sup` and `retinue_poe2_sup` subtrees. Game services have their own permanent
children. Queue workers are monitored; worker failure becomes a request error while the owner
survives. Systemd adds VM-level restart for explicitly installed user services.

### Memory Ownership

Keep temporary decoding and projection work in bounded workers, not long-lived service heaps.
Resolution audits coalesce updates into one current worker and one pending refresh; failures
retain the last good registry. Source freshness checks use the serialized source-worker queue.
Hot migrations must not eagerly reload a cache already present in state.

Build replies expire through a nearest-deadline timer and retain at most 128 entries;
saved groups are separate and never evicted. Build, resolution and source services use OTP's
30-second idle hibernation to release spare heap capacity. This startup setting needs a service
restart when first introduced; hot-loading callback code does not retrofit it.
Measure RSS, live state and allocated heaps separately before changing allocator settings.

## Hot Update

[term:BUILD_ID]: retinue_build:artifact_id/0

Both explicit and automatic updates use `retinue_hot_update`:

1. Read and validate `retinue_core`, `wfcore`, `poe2core`, and `retinue_daemon` BEAM bundles.
2. Reject duplicate module identities.
3. Skip unchanged modules by BEAM MD5.
4. Suspend affected registered stateful services.
5. Load changed modules, invoke [`code_change/3`](https://www.erlang.org/doc/apps/stdlib/gen_server.html#c:code_change/3), and resume services.
6. Restart the local socket API when its accept loop cannot leave old code.
7. Commit the new build identity only after successful load and migration.

Migration targets come from supervisor child definitions. Failed or interrupted migrations stay
pending until the affected services restart from persisted state. Identical BEAMs alone never
prove migration success.

Each staged root contains `BUILD_ID`. The daemon checks it every five seconds and reads BEAMs from
its stable update root. Repository builds use [`dev/` or `prod/`](workflows.md#targets); `RETINUE_UPDATE_ROOT` can select
another stable installation root. This supports same-version updates without a client request.

`retinue daemon update` sends the same typed hot-update request explicitly. No updater module is
bootstrapped through remote evaluation. Standard versioned [`.appup`](https://www.erlang.org/doc/apps/sasl/appup.html)/relup artifacts remain the
path for module deletion and complex release ordering via `retinue daemon update --release RELEASE`.

## Ownership Rule

Daemon modules return data and never print or halt. CLI modules parse options, submit typed
requests, and render replies. MCP modules translate JSON schemas into the same requests.
`retinue_core` contains platform helpers; `wfcore` contains Warframe shared contracts and helpers.
Neither library owns service state.
