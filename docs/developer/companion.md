# Companion Architecture

Ownership:

- `game_observer` owns Warframe process/build identity, read-only memory access,
  DBWIN classification, and typed evidence.
- `observation` owns shared sampling and evidence recording; `wfinspect` owns
  research commands, output and recording limits.
- `wfcompanion` owns capture timing, OCR fallback, incidents, and daemon publication.
- `wfcompanion` overlay owns scenes, interaction, focus gating, layout,
  rendering, and display caches.
- `retinue-daemon` owns canonical player snapshots, persistence, queries, Market
  access, asset resolution, and relic calculations.

Overlay state stays native. Canonical player state stays in the daemon.

## Native Modules

- `runtime/`: service ownership, launched-game monitor, game sessions, and bounded jobs.
- `runtime/inbox.rs`, `runtime/presentation.rs`: bounded local mailboxes and HUD/scene messages.
- `game_observer/`: process identity, scoped memory reads, explicit movie diagnostics,
  and caller-driven UI transition tracking.
- `observation/`: bounded GEP sampler, capture/replay, and private recording files,
  shared by companion and inspector. Readers do not depend on this layer.
- `observer.rs`: bridge lifecycle and observation policy.
- `observation/debug_output/`: shared DBWIN helper ownership and subscriber fan-out.
- `inventory.rs`: ordered HTTP decode and publication.
- `game_observer/inventory.rs`, `inventory/refresh.rs`: scoped native inventory reads.
- `inventory/observation.rs`: session identity, acquisition times and publication ordering.
- `daemon.rs`, `daemon/`: local JSON-lines client, bounded outbound queue,
  reconnect, replay, and request routing.
- `capture.rs`, `relic.rs`: KWin window capture, crop detection, OCR, and relic
  scene construction.
- `relic/lifecycle.rs`: context cancellation and bounded scene workers.
- `relic/evidence.rs`: armed evidence, capture phases and terminal reports.
- `work.rs`: caller-owned cooperative deadlines, cancellation and I/O budgets.
- `crates/retinue-overlay/src/focus.rs`: exact game window and process gate.
- `overlay/runtime.rs`: layer shell, SHM buffers, frame callbacks, input, focus,
  and damage.
- `overlay/renderer.rs`: fonts, static frame cache, dispatch, and previews.
- `overlay/assets.rs`: background image preparation and decoded-image budget.
- `overlay/scene.rs`: top-level scene and presentation state.
- `overlay/screens/`: complete screen painting and hit regions.
- `ui/geometry.rs`: game hit regions and render output.
- `crates/retinue-overlay`: shared Taffy layout, geometry, painter and Blend2D
  ownership; also portal shortcuts and the PoE2 layer-shell host. Screen models,
  composition and actions belong to each companion.

Observer work runs off the UI thread. New collectors publish a separate source
namespace instead of extending one untyped payload.

HTTP discovery validates the identified executable's manager, queue accessors and
response callback before sampling. Conflicting bindings fail explicitly; direct
and indirect response buffers are checked independently.

The HTTP sampler copies candidate bytes and waits 7 ms after each iteration;
JSON parsing and publication run on a separate decoder. Its FIFO
holds at most 16 samples and 16 MiB of retained allocations. Overflow evicts oldest
samples, reports gaps, and retries retained response buffers when capacity returns.
Transient responses can still be lost. Accepted-payload deduplication survives
retries so an old retained response cannot undo a newer accepted snapshot.

Collector reports include sampling work, actual intervals, decode time, queue wait
and overflow counters. They accompany existing reports rather than generating
periodic player revisions. `wfcli companion status --json` exposes the full report.
`wfcli diagnostics status` requests current counters without a player publication.
`wfcli doctor` summarizes live adapter evidence. Reporting rules are shared across
games; see [Health evidence](health.md).

Account seeds refresh once per second. The reader derives registry/profile fields
and getter addresses from native code, validates live vtables, and selects the
primary profile by its identity getter. It never calls game functions. Discovery
failures remain separate from HTTP capture. Reports include account availability,
attempts and failures; incidents record failure/recovery transitions without identifiers.

Decoded publication retains the latest HTTP inventory and native scopes (32 MiB
combined accounted allocations), plus a separate account seed; metadata retains one state (8 MiB). Replaced
snapshots and rejected entries are counted; raw HTTP responses are not coalesced.
Producers never wait for consumer capacity.

Presentation retains only the player's HUD fields, not the inventory. Its inbox
reserves 32 messages/8 MiB for ordered scene transitions and controls, separate
from 64 status/asset messages/128 KiB. Status and asset updates coalesce by identity;
scene transitions and interaction toggles retain order. Each tick handles at most
32 events before returning to Wayland. Unmatched asset refreshes retain at most
128 entries/128 KiB. Overflow is reported; these are queue budgets, not RSS limits.
Game exit signals runtime shutdown directly, independently of presentation capacity.

DBWIN intake retains 256 records/1 MiB, with read-time timestamps and dropped-record counters.
Overflow discards oldest records and invalidates relic context. The inspector uses the same
bridge and reports queue loss. Relic intake keeps 64 commands/64 KiB in order; eight reserved
entries/8 KiB hold the two workers' completion messages and a coalesced gap marker. Rejection
reports an incident and invalidates context instead of leaving an unreliable scene active.

Resource stacks (`MiscItems`), blueprint stacks (`Recipes`) and foundry jobs
(`PendingRecipes`) refresh from native state once per second on a separate worker.
Registry, type, field and quantity-encoding bindings are discovered from the
executable once per reader. Named serializers must agree; ambiguous or changed
structures produce a discovery error. Inventory and metadata share registry and
resource layout discovery. Descriptor construction and independent native getters
must agree on resource fields. The profile is selected by descriptor and
holder backlink, not its registry key. Reads are bounded and checked for concurrent mutation. A refresh
must match the full inventory's `LastInventorySync` and start after its capture.
Companion publishes `inventory_http` and `inventory_native` separately; the daemon
owns [canonical reconciliation](daemon.md#market-and-player-data). The decoder retains
baseline identity/timing and native deduplication state, not a mutable full inventory.
HTTP `InventoryChanges` deltas are never summed.

`TransitionTracker` has no timer. A future bounded registry watch feeds it at a
profiled cadence; production must not run exploratory heap scans or add periodic
sampling without frametime measurements. DBWIN events remain the immediate path.

## Layout

`UiTree<K>` is the semideclarative layout layer. Screens declare named leaves,
rows, columns, grids, stacks, dimensions, spacing, alignment, and clipping.
Taffy resolves absolute and content bounds; screen code never walks parent
origins or computes draw rectangles.

Taffy does not paint. Each screen consumes named bounds and issues exact
Blend2D operations. This preserves predictable draw order and keeps visual
logic beside its screen. Add shared components only after two screens need the
same composition.

Each screen owns:

- typed display model and presentation state;
- top-level `UiTree`;
- painting and animation;
- hit targets;
- preview fixture and pixel tests.

Screens do not import sibling screens. `scene.rs` dispatches them. Runtime uses
`ScreenOutput` from the cached render and has no screen-specific geometry.

Use function-specific borrowed input structs when a call carries one semantic
operation with many fields. Return normal Rust values. Output pointers belong
only at FFI boundaries. Thin Blend2D wrappers use reusable image, rectangle,
circle, and mask value types while preserving the underlying C ABI.

## Rendering

The Wayland surface uses `wl_shm::Format::Argb8888`. On little-endian Linux its
bytes are premultiplied BGRA, matching Blend2D `PRGB32`.

Static scene geometry is rasterized once into a cached frame. Animation:

1. restores its dirty rectangle from the static cache;
2. draws only dynamic primitives;
3. submits the same rectangle with `wl_surface.damage_buffer`.

New, resized, or invalidated buffers receive one full static-frame copy.
`wl_buffer.release` controls buffer reuse; frame callbacks control submission
timing; damage controls compositor repainting.

One background worker reads and decodes scene assets, sharing immutable pixels
by digest. Each prepared scene is limited to 64 MiB, each image to 16 MiB.
The prior scene can remain resident while its replacement loads. Apply only the
latest asset revision, then invalidate both the static frame and every surface
buffer's cache key. File reads and image decoding must not run on Wayland's thread.

Blend2D remains synchronous because icons and fontdue glyph masks are borrowed
for each draw call. Its asynchronous multithreaded context requires owned
source lifetimes through `Painter::finish` and is not useful for the current
cached workload.

Text measurement, wrapping and painting share fractional glyph advances and
kerning in `retinue-overlay::painter`. Round only final raster positions, never
each character's advance; per-character rounding accumulates spacing errors.
Use `text_width` and `wrap_text` for layout rather than a separate estimate.

Passive mode has no pointer region. Interactive mode captures the surface;
screen hit regions route actions. Hit rectangles use buffer pixels; convert
Wayland surface-local pointer positions with the rendered frame's scale before
testing them. Layer-shell margins remain zero so a persistent pointer position
cannot fall into an uncapturable compositor gap.

Both hosts apply `retinue_overlay::layer::OverlayLayerState` before remapping.
Unmapping resets every layer-shell property, including the layer itself. Restore
the complete role state, commit without a buffer, then wait for configure before
drawing. Do not duplicate partial compositor setup in individual games.

## Relic Pipeline

The trigger owner handles close, dismissal and game-stop without waiting for
capture, OCR or daemon requests. At most two scene workers run, with one latest
pending job. Every result carries a cancellable context through UI acceptance;
workers also check cancellation between stages. Reward expiry is measured from
the original event, not completion of a delayed request.
OCR children have a ten-second deadline, bounded stdout/stderr and are killed
and reaped when their scene is cancelled. KWin method and pixel-read waits are
limited to three seconds per blocking operation.

Reward flow:

Scaleform bindings are discovered from the executable on an owned worker at game
attach and cached for that session. Missing or ambiguous bindings use OCR; a new
session retries discovery. Captures store validated bindings for offline replay.

1. Deduplicate reward debug events.
2. Traverse the reward movie's typed Scaleform registry immediately.
3. Retry incomplete/loading labels every 100 ms within the 650 ms window;
   use complete names immediately in returned order.
4. If extraction fails or times out, capture at the 650 ms stabilization deadline and OCR names.
5. Resolve labels and one quote batch through the daemon.
6. Resolve ducats, vault state, player state, set graph, and visible assets.
7. Render complete cards.

Memory order is provisional until verified across one-to-four-player reward screens. Incidents log
raw memory order; the overlay displays that same order, making mismatches visible during testing.

`wfcli companion capture arm relic-reward` records an image, bounded typed Scaleform
reads, executable bindings, per-movie completeness, and timing metadata for
one event. The arm expires after 30 minutes, cancellation, or game exit.
Triggered evidence runs independently of scene workers and price lookup, with at
most two owned jobs. Dismissing the scene does not cancel it. Screenshot capture
still waits for stabilization; memory and image are not an atomic snapshot.
Memory acquisition finishes and its session is revalidated before file output.
Captures use new owner-only directories and never overwrite an existing recording.
Capture-only read recording reuses the normal typed walker and preserves overlapping
first-seen bytes. Registry acquisition avoids heap scanning; unavailable bindings fall
back to the forensic graph with an explicit reason. One budget spans reads and writes. Admission
precedes worker creation; cancellation and shutdown stop further work between
operations, then join the workers. Byte reservations count attempted I/O, not
physical traffic. A reserved terminal report records incomplete output. Deadlines
are cooperative: they cannot interrupt a kernel-blocked filesystem operation.
Changing a scene must not cancel an explicit recording; cancelling capture, ending
the game session or shutting down companion must.

New blocking jobs must own their children, enforce finite work/output limits and
check cancellation within long loops. Do not detach work to hide shutdown latency.

Selection flow:

1. Detect selection open/close events.
2. Probe the bounded Scaleform registry and classify dynamic era labels.
3. If validation fails, capture and OCR the era at the 500 ms stabilization deadline.
4. Ask the daemon to rank owned relics.
5. Render immediately from cached data and refresh prices asynchronously.

`Forma Blueprint` is local because it is not tradable. Dynamic image behavior
is documented in [`assets.md`](assets.md); inventory indexing is documented in
[`player-data.md`](player-data.md).

## Local Contract

Transport is newline-delimited JSON over the shared owner-only
`$XDG_RUNTIME_DIR/retinue/daemon.sock` socket. Clients send `hello` first with envelope 2 and
mandatory `game: "warframe"`. PoE2 uses the same endpoint with `game: "poe2"` and its separate
`session` interface. The connection stays game-scoped. Envelope version covers framing and
handshake. Exact Warframe interface
versions cover `datasets`, `player`, `game_metadata`, `worldstate`, `notifications`, `market`,
`overframe`, `relics`, `assets`, `builds`, `build_stats`, `diagnostics`, and `checks` independently.
Clients send only required interfaces and request optional features.

Requests:

- `get`, `subscribe`, `unsubscribe`
- `publish`
- `market_resolve`
- `market_quote`, `ducat_inventory` (local item identity, parent status and cached prices)
- `relic_context`
- `relic_planner`
- `relic_recommendations`
- `asset_resolve`
- build, notification, account, cache, and diagnostics operations owned by their
  corresponding interface group

Events:

- `dataset`: replacement subscription snapshot
- `command`: overlay diagnostic command
- `asset`: refreshed asset descriptor
- `companion_diagnostics`: correlated live status, watch, credit, cancellation or stop request

Companion subscribes to player data with `view: "hud"`: game phase/PID and DBWIN
active/line-count fields only. Inventory updates do not trigger that subscription.
Explicit `get` still returns the full dataset. Metadata-only subscribers use
`include_data: false`; projections happen before delivery to socket workers.

Daemon input frames are capped at 8 MiB; companion receive frames at 64 MiB.
Partial frames survive cancelled reads. Malformed JSON, oversized frames and
EOF before a newline close the connection and fail pending requests.
Writes retain their offset across input handling so a stalled peer cannot block
controls. Outbound, disconnected-event and replay stores each allow at most 64
entries and 32 MiB of accounted values; at most 64 RPCs await replies. Unsent
absolute state coalesces only for registered snapshot sources; capture outcomes and diagnostic reports
remain separate events. Overload is rejected and reported, not silently discarded.
Socket parent mode is `0700`; socket mode is `0600`. Same-user clients are trusted. Player data does not enter Erlang
distribution or terminal formatting contracts.

`companion.command`, `companion.diagnostics` and `diagnostics.report` are optional negotiated features.
Breaking framing changes the envelope; breaking domain semantics changes only
that interface version. Release applications remain lockstep and carry no legacy
wire adapters.

Runtime diagnostics reuse collector counters in the observer loop. At most four inventory
watches run for 1-1800 seconds at one-second intervals, with 16 terminal jobs retained.
The socket owner tracks at most 16 requests and requires an initial reply within five seconds.
One sample per watch awaits consumption credit; missed samples are counted, never replayed.
Completion bypasses credit. Native command and credit queues each hold 16 entries/8 KiB;
each socket has 32 reply slots, with a 32 KiB retained-value budget per reply. Closing that
socket cancels its watches. Diagnostics never enter player storage or reconnect replay.

## Lifecycle

Launch mode keeps companion tied to the Steam child and gives the inventory
collector ptrace ancestry. Standalone mode is managed by the CLI.

`runtime::Core` owns observation, relic work, the daemon connection and shortcut
service. Shutdown cancels scene work, closes the shortcut session, releases DBWIN,
and joins observation, scene and evidence workers. The daemon stays available
until producers stop, then attempts a bounded publication flush and joins.
RPC waits cancel during quiescence; final publications remain accepted until
producer shutdown. An unavailable overlay disables interaction but leaves
observation running until game exit or explicit stop; restart to restore the overlay.
SIGTERM, Ctrl+C, partial startup and error exits follow the same cleanup path. The launched-game
monitor is separate: stopping it never terminates the game.

Game sessions include PID, process start time, executable identity and a local
generation. Readers share successful executable discovery. Replacing a session
invalidates its scenes and retires its collectors before accepting new events.
Check the session around native reads; a bare PID cannot identify delayed work.

Every resource-owning worker needs an owner and a join path. Close admission
before draining jobs, report terminal outcomes, and keep evidence lifetime
separate from scene cancellation. Do not detach workers or add per-reader timers.
Keep partial-write state outside `select!`: Tokio's `write` is cancellation-safe,
but `write_all` is not ([contract](https://docs.rs/tokio/latest/tokio/macro.select.html#cancellation-safety)).

The process owns one incident writer and joins it last. Its nonblocking queue
holds at most 128 bounded records, with 32 slots reserved from informational
traffic. The writer handles deduplication, rotation and disk I/O, reports overflow,
and drains accepted records on shutdown. Logging must not write files from
collector or UI threads.

Capture the executable path at startup and use `retinue_client::executable_path()` for sibling
tools. Prefix activation can move the running executable's `/proc/self/exe` path; helper
lookup must continue using the installed prefix.

Companion starts or reconnects to the daemon without passing Proton loader
variables into BEAM. The startup helper runs asynchronously with a 30-second
deadline and is killed and reaped on timeout or shutdown.
Reconnect replays latest observations for every owned
namespace. An active companion connection keeps an implicitly started daemon
alive.

Debug builds show the compact HUD during startup and hide it when Warframe starts.
Release builds start with it hidden. Explicit `hud show` and `hud hide` override
automatic visibility. Hidden mode keeps the transparent layer surface mapped
after its first scene because KWin may not present a detached and reused layer role reliably.
No idle render loop runs.

## Development

Build and test commands live in [`workflows.md`](workflows.md). The
[`wfinspect` guide](../wfinspect.md) covers live/offline queries, captures,
resource extraction, script analysis, and event subscriptions.

Development builds reload by default. Build with `make dev-companion` and use Steam launch options:

```text
/path/to/retinue/wfcompaniond launch -- %command%
```

Later `make dev-companion` builds replace companion in the same process once
relic screens, interaction and captures are idle. The game stays running;
inventory ordering and pending snapshots survive. A brief observation gap is
reported by `reload.exec` and `reload.resumed` in the incident log. A rejected
replacement resumes the old code; incompatible handoff changes require restarting
the launch wrapper. Set `WFCOMPANION_DEV_RELOAD=0` to disable reload.

`retinue-client` shares signal handling, child ownership, candidate validation and
same-PID replacement across companions. Each game owns its idle gate and handoff
state. PoE2 drains publication and retains unacknowledged/coalesced observations;
ARC's launch stub retains only child ownership. Their development builds also
reload by default, controlled by `POE2COMPANION_DEV_RELOAD` and
`ARCCOMPANION_DEV_RELOAD` respectively.

Production builds require a restart after staging. Replacing a running executable
can invalidate KWin screenshot authorization until it restarts or reloads.

Preview commands live in the [user companion guide](../companion.md#previews).
Provider reference adapters share the [`ui-reference` tool](../../tools/ui-reference/README.md).

Upstream behavior and safety evidence:

- [`alecaframe-overlay-catalog.md`](alecaframe-overlay-catalog.md)
- [`overwolf-alecaframe-overlay-research.md`](overwolf-alecaframe-overlay-research.md)
