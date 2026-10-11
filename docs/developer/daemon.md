# Daemon Architecture

`retinue_daemon` hosts shared runtime services and separate game supervisors. The public binary
is `retinue-daemon`. Warframe services own fetching, worldstate, catalogs, queries, player state,
Market, Forma planning, and watches. PoE2 services own normalized local-log session evidence.
`retinue_cli` composes command scopes; the `wfcli` adapter owns Warframe arguments, MCP framing,
and formatting. Native observation stays in each game's companion.

## Lifecycle And Transport

- Clients connect to node `retinue_daemon@localhost` through `retinue_client`.
- Distribution binds to IPv4 loopback and uses the invoking user's Erlang cookie.
- OTP 29 dynamic hidden client nodes do not listen for incoming distribution connections.
- Client/daemon requests use typed `gen_server:call({retinue_daemon, Node}, Request, Timeout)`.
- Game RPC uses `{game, Game, Request}` or a `game` field on asynchronous requests. Platform
  status keeps domain state under `games.<game>`; unscoped domain requests are rejected.
- Production code contains no direct `rpc:*`, `erpc:*`, or remote `spawn*` evaluation.
- Concurrent startup tolerates another client winning the race.
- Implicit startup uses idle shutdown; explicit startup reapplies persistent or requested idle
  policy to a running daemon.
- Watches, queued work, and connected companions hold an activity lease.
- Forma plans are serialized because one planner already uses internal CPU parallelism.

The daemon changes its working directory to `$XDG_STATE_HOME/retinue` during application startup.
This keeps crash dumps and relative runtime state valid while a staged release directory is
replaced.

## Build Identity

Each transport has a stable handshake envelope and independently versioned interface groups.
Clients require exact versions for groups they use and negotiate optional features. Artifact
flavor (`dev` or `prod`) prevents one environment from starting or updating the other. Build
identity is SHA-256 over sorted module names and BEAM MD5 values for `retinue_core`, `wfcore`,
and `retinue_daemon`.

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

One-off commands submit a monitored one-shot subscription. The daemon queues work, sends exactly
one `{retinue_daemon, Ref, Reply}` message, and removes the request. Persistent watches use the same
shape but stay registered.

The local client relay is monitored across distribution. Closing a terminal, killing a client, or
MCP cancellation produces `DOWN`; queued or active query, market, catalog, and planning work is
released without polling. Shared worldstate fetches may continue when other clients or cache
refresh still need them.

Companion diagnostic streams use consumption credit and expire. The local socket owner
correlates requests to one companion and monitors the CLI relay; disconnects cancel the
watch instead of resubscribing it. See [companion transport](companion.md#local-contract).

## Native Game Scopes

All native clients connect to `$XDG_RUNTIME_DIR/retinue/daemon.sock`, with an owner-only
`/tmp/retinue-UID/daemon.sock` fallback. `RETINUE_DAEMON_SOCKET` can override the endpoint.
Local envelope 2 requires `game` in the initial `hello`: `warframe` or `poe2`. The connection
keeps that scope for requests, subscriptions, publications, and cleanup. Missing or unsupported
games fail negotiation; a client cannot access another game's interfaces through its session.

Warframe negotiates its existing domain interfaces. PoE2 negotiates `session` version 1 for
`get`, `publish`, and subscriptions. Interface versions and optional features are game-scoped;
the shared framing envelope is not a declaration of equivalent capabilities across games.

`retinue_paths` owns platform XDG roots and per-game namespaces. Warframe's `wfcli_paths` remains
an adapter. `RETINUE_BUILD_FLAVOR`, `RETINUE_INSTALL_ROOT`, and `RETINUE_UPDATE_ROOT` identify
the staged runtime; game-specific settings keep their existing names.

## Worldstate And Catalogs

`wfcli_worldstate_service` caches decoded snapshots by cache path and indexes by normalization
options. With active watches, one global timer polls worldstate; every due watch evaluates against
the same parsed snapshot. Fetch failure retains the last good snapshot and marks delivery stale.

`wfcli_exports_store` serializes official export and Codex queries. Catalog keys include source
directory, selected files, entity kind, modification time, and size. `wfcli_source_manager`
prepares missing managed data and queues periodic stale refreshes. Explicit custom source
directories are never modified. Refresh writes use atomic rename; active readers retain complete
old files and later catalog signatures rebuild cached entities.

Query text reaches the daemon uncompiled. `wfcli_query_parse` builds the AST; entity modules expose
typed fields and retain raw data paths where a parser has not added semantics.
Catalog query replies contain only the requested `slice` plus `total` and `shown` counts;
the complete matching set stays inside the daemon.

## Market And Player Data

`wfcli_market_service` owns Warframe Market manifests, base and variant quote TTLs, persistence,
and coalescing. `wfcli_market_account_service` serializes authenticated order mutations;
`wfcli_market_presence_service` owns account status. Public and authenticated HTTP work share
`wfcli_market_limiter`. API modules handle wire normalization and caches use atomic persistence.

`wfcli_player_service` owns canonical local observations by source namespace. `retinue_local_api`
accepts owner-only Unix socket clients, including `wfcompanion`, and removes subscriptions on
disconnect. Persisted observations do not keep the daemon alive.

`inventory_http` retains the last full response; `inventory_native` retains absolute
resource, blueprint and foundry collections. `wfcli_player_inventory` validates and
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

Native clients use request IDs over newline-delimited JSON on the Unix socket. Each connection
can run independent reads concurrently; market mutation and rate-limited fetches remain serialized
by their owning service. A representative 80 KB relic-planner response costs about 1 ms to encode
or decode on BEAM, so JSON is retained until profiling shows transport encoding is material.
GUI and companion can reconcile current decode failures through the optional diagnostics feature;
the daemon stores them with its own name, metadata, and asset failures.

Native sessions monitor their backend owners. A backend restart closes affected sessions;
clients reconnect and reconcile snapshots instead of retaining dead subscriptions or requests.

## PoE2 Sessions

`retinue_poe2_service` accepts bounded, validated batches of local-log evidence. Source identity,
generation, and byte cursor prevent duplicate replay; a changed source or generation resets the
projected session. Persistence completes before publication is acknowledged. Status retains the
last known source, cursor, current projection, and up to 128 events in
`$XDG_CACHE_HOME/retinue/games/poe2/session.json`.

Area level is separate from character level. Character events retain their named actor; they do
not establish which character belongs to the local player. Log timestamps have no timezone and
remain source-local. A stored snapshot is not proof that the game is currently running.

## Build Groups

`wfcli_build_service` owns saved groups and immutable member snapshots. Mutations are acknowledged
only after an atomic, synchronous state-file save. A rejected state file remains untouched and
group operations report the error. Durable groups live at
`$XDG_DATA_HOME/retinue/games/warframe/builds.term`. Refetchable source revisions and plan results
live separately in XDG cache.

Physical targets are resolved from current player equipment when read or planned. Plans are keyed
by group revision, target fingerprint, and source catalog identity; completion rechecks all three.
Missing targets retain their groups but require retargeting before calculation.

Build-stat requests share the monitored build worker pool but neither depend on Overframe nor
cache mutable player results. `wfcli_stat_buildset` resolves slot-keyed owned selectors or inline
normalized builds. `wfcli_stat_profiles` validates reviewed attacks against PublicExport;
`wfcli_stat_rules` and `wfcli_stat_upgrades` describe reviewed mechanics and ranged mods.
`wfcli_stat_effects` compiles source/target-filtered contributions and their dependencies.
`wfcli_stat_math` evaluates numeric
stages; `wfcli_build_stats` coordinates inputs and reports coverage. CLI and native clients use
the `build_stats` interface. Clients never interpret raw game slot order or apply combat formulas.
`scalar/2` evaluates an ordered upgrade bucket, stopping at its first `SET`.
`scalar_updates/2` applies successive accumulator writes; later `SET` writes replace earlier ones.
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
`wfcli_warframe_sup` and `retinue_poe2_sup` subtrees. Game services have their own permanent
children. Queue workers are monitored; worker failure becomes a request error while the owner
survives. Systemd adds VM-level restart for explicitly installed user services.

## Hot Update

Both explicit and automatic updates use `retinue_hot_update`:

1. Read and validate `retinue_core`, `wfcore`, and `retinue_daemon` BEAM bundles.
2. Reject duplicate module identities.
3. Skip unchanged modules by BEAM MD5.
4. Suspend affected registered stateful services.
5. Load changed modules, invoke `code_change/3`, and resume services.
6. Restart the local socket API when its accept loop cannot leave old code.
7. Commit the new build identity only after successful load and migration.

Migration targets come from supervisor child definitions. Failed or interrupted migrations stay
pending until the affected services restart from persisted state. Identical BEAMs alone never
prove migration success.

Each staged root contains `BUILD_ID`. The daemon checks it every five seconds and reads BEAMs from
its stable update root. Repository builds use `dev/` or `prod/`; `RETINUE_UPDATE_ROOT` can select
another stable installation root. This supports same-version updates without a client request.

`retinue daemon update` sends the same typed hot-update request explicitly. No updater module is
bootstrapped through remote evaluation. Standard versioned `.appup`/relup artifacts remain the
path for module deletion and complex release ordering via `daemon update --release RELEASE`.

## Ownership Rule

Daemon modules return data and never print or halt. CLI modules parse options, submit typed
requests, and render replies. MCP modules translate JSON schemas into the same requests.
`retinue_core` contains platform helpers; `wfcore` contains Warframe shared contracts and helpers.
Neither library owns service state.
